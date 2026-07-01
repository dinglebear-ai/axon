//! Source document preparation entry point.

use std::collections::HashSet;

use axon_api::source::{
    ChunkId, ChunkLocator, CleanupKey, ContentRef, MetadataMap, PreparedChunk, PreparedDocument,
    SourceError, SourceRange, SourceWarning,
};

use crate::chunk::DocumentChunk;
use crate::chunk_router::ChunkRouter;
use crate::prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use crate::profile::ChunkingProfile;
use crate::{code, markdown, metadata, schema, session, text, transcript};

#[derive(Debug, Default, Clone)]
pub struct DocumentPreparer {
    router: ChunkRouter,
}

impl DocumentPreparer {
    pub fn prepare(
        &self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        let profile = request
            .profile
            .map(Ok)
            .unwrap_or_else(|| self.router.route(&request.document))?;
        let text = inline_text(&request.document.content)?;
        let chunks = build_chunks(profile, text, request.document.structured_payload.as_ref());
        let prepared_chunks = prepare_chunks(&request, profile, chunks);
        let document = PreparedDocument {
            document_id: request.document.document_id,
            source_id: request.document.source_id,
            source_item_key: request.document.source_item_key,
            generation: request.generation,
            canonical_uri: request.document.canonical_uri,
            prepare_version: "axon-document-pr8".to_string(),
            chunking_profile: profile.as_str().to_string(),
            chunking_method: profile.as_str().to_string(),
            chunks: prepared_chunks,
            metadata: request.document.metadata,
            cleanup_keys: Vec::<CleanupKey>::new(),
            graph_refs: Vec::new(),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::<SourceWarning>::new(),
            errors: Vec::<SourceError>::new(),
        };
        validate_prepared_document(&document)?;
        Ok(PrepareSourceDocumentResult { document })
    }
}

fn inline_text(content: &ContentRef) -> Result<&str, String> {
    match content {
        ContentRef::InlineText { text } => Ok(text),
        ContentRef::InlineBytes { .. } => Err("inline bytes are not prepared yet".to_string()),
        ContentRef::Artifact { .. } => Err("artifact content is not prepared yet".to_string()),
        ContentRef::External { .. } => Err("external content is not prepared yet".to_string()),
    }
}

fn build_chunks(
    profile: ChunkingProfile,
    text: &str,
    structured_payload: Option<&serde_json::Value>,
) -> Vec<DocumentChunk> {
    match profile {
        ChunkingProfile::CodeSymbol => code::code_symbols(text),
        ChunkingProfile::CodeManifest => code::code_manifest(text),
        ChunkingProfile::MarkdownSections => markdown::markdown_sections(text),
        ChunkingProfile::HtmlArticle => markdown::html_article(text),
        ChunkingProfile::PlainTextWindows => text::plain_text_windows(text),
        ChunkingProfile::TranscriptSegments => transcript::transcript_segments(text),
        ChunkingProfile::StructuredRecords => {
            metadata::structured_records(text, structured_payload)
        }
        ChunkingProfile::ApiSchema => schema::api_schema(text, structured_payload),
        ChunkingProfile::ToolOutput => transcript::split_on_nonempty_lines(text, "tool_output"),
        ChunkingProfile::SessionTurns => session::session_turns(text),
        ChunkingProfile::AtomicMetadata => metadata::atomic_metadata(text),
    }
}

fn prepare_chunks(
    request: &PrepareSourceDocumentRequest,
    profile: ChunkingProfile,
    chunks: Vec<DocumentChunk>,
) -> Vec<PreparedChunk> {
    let len = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, mut chunk)| {
            chunk
                .metadata
                .insert("chunking_profile".to_string(), profile.as_str().into());
            let path = chunk
                .metadata
                .get("original_path")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| request.document.path.clone());
            let chunk_id = ChunkId::from(format!("{}:{idx:04}", request.document.document_id.0));
            PreparedChunk {
                chunk_id: chunk_id.clone(),
                chunk_key: format!("{}:{idx:04}", request.document.document_id.0),
                document_id: request.document.document_id.clone(),
                chunk_index: idx as u32,
                content_hash: simple_hash(&chunk.content),
                embedding_text: None,
                chunk_locator: ChunkLocator {
                    canonical_uri: request.document.canonical_uri.clone(),
                    path,
                    heading_path: chunk.heading_path,
                    symbol: chunk.symbol,
                    range: chunk.range.clone(),
                },
                source_range: chunk.range,
                content_kind: request.document.content_kind,
                title: chunk.title.or_else(|| request.document.title.clone()),
                graph_refs: Vec::new(),
                parent_chunk_id: None,
                previous_chunk_id: (idx > 0).then(|| {
                    ChunkId::from(format!("{}:{:04}", request.document.document_id.0, idx - 1))
                }),
                next_chunk_id: (idx + 1 < len).then(|| {
                    ChunkId::from(format!("{}:{:04}", request.document.document_id.0, idx + 1))
                }),
                metadata: merge_metadata(&request.document.metadata, chunk.metadata),
                content: chunk.content,
            }
        })
        .collect()
}

pub(crate) fn validate_prepared_document(document: &PreparedDocument) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut chunk_ids = HashSet::new();
    let mut chunk_keys = HashSet::new();

    if document.chunks.is_empty() {
        errors.push("prepared document has no chunks".to_string());
    }

    for chunk in &document.chunks {
        if !chunk_ids.insert(chunk.chunk_id.clone()) {
            errors.push(format!("duplicate chunk id: {}", chunk.chunk_id.0));
        }
        if !chunk_keys.insert(chunk.chunk_key.clone()) {
            errors.push(format!("duplicate chunk key: {}", chunk.chunk_key));
        }
        if chunk.content.trim().is_empty() {
            errors.push(format!("empty content after trim: {}", chunk.chunk_id.0));
        }
        range_errors("source_range", &chunk.source_range, &mut errors);
        range_errors("locator range", &chunk.chunk_locator.range, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn range_errors(label: &str, range: &SourceRange, errors: &mut Vec<String>) {
    if starts_after(range.line_start, range.line_end) {
        errors.push(format!("{label} line_start > line_end"));
    }
    if starts_after(range.byte_start, range.byte_end) {
        errors.push(format!("{label} byte_start > byte_end"));
    }
    if starts_after(range.char_start, range.char_end) {
        errors.push(format!("{label} char_start > char_end"));
    }
    if starts_after(range.time_start_ms, range.time_end_ms) {
        errors.push(format!("{label} time_start_ms > time_end_ms"));
    }
}

fn starts_after<T: Ord>(start: Option<T>, end: Option<T>) -> bool {
    start.zip(end).is_some_and(|(start, end)| start > end)
}

fn merge_metadata(doc: &MetadataMap, mut chunk: MetadataMap) -> MetadataMap {
    for (key, value) in doc.iter() {
        chunk.entry(key.clone()).or_insert_with(|| value.clone());
    }
    chunk
}

fn simple_hash(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

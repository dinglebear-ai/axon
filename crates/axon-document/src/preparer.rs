//! Source document preparation entry point.

use std::collections::HashSet;

use axon_api::source::{
    ChunkId, ChunkLocator, CleanupKey, ContentRef, MetadataMap, PreparedChunk, PreparedDocument,
    Severity, SourceItemKey, SourceRange, SourceWarning,
};

use crate::chunk::DocumentChunk;
use crate::chunk_router::ChunkRouter;
use crate::parse::{DocumentParse, parse_document};
use crate::prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use crate::profile::ChunkingProfile;
use crate::source_range::{SourceRangeBounds, bounds_for_text, validate_source_range};
use crate::{code, markdown, metadata, schema, session, text, transcript};

#[derive(Debug, Default, Clone)]
pub struct DocumentPreparer {
    router: ChunkRouter,
}

impl DocumentPreparer {
    pub fn prepare(
        &self,
        mut request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        // Activate axon-parse on the acquisition path: when the caller did not
        // pre-supply parse facts, parse the document here so parser-driven chunk
        // routing and graph candidates actually flow. Callers that already carry
        // facts (or explicitly forced a profile) keep their supplied artifacts.
        let parse = if request.parse_facts.is_empty() {
            parse_document(&request.document)
        } else {
            DocumentParse::default()
        };
        merge_parse_artifacts(&mut request, &parse);

        let profile = match request.profile {
            Some(profile) => profile,
            None => parse
                .routed_profile()
                .map(Ok)
                .unwrap_or_else(|| self.router.route(&request.document))?,
        };
        let content = content_text(&request.document);
        let bounds = bounds_for_text(&content.text);
        let effective_profile = content.force_profile.unwrap_or(profile);
        let build = build_chunks(
            effective_profile,
            &content.text,
            request.document.structured_payload.as_ref(),
            &request.document.source_item_key,
        );
        let chunks = build.chunks;
        let parser_stamp = (!parse.parser_id.is_empty() && parse.parser_id != "none")
            .then_some((parse.parser_id.as_str(), parse.parser_version.as_str()));
        let prepared_chunks = prepare_chunks(&request, effective_profile, chunks, parser_stamp);
        let mut warnings = request.warnings;
        warnings.extend(content.warnings);
        warnings.extend(build.warnings);
        let mut document_metadata = request.document.metadata;
        document_metadata.insert(
            "normalized_line_count".to_string(),
            serde_json::json!(bounds.line_count),
        );
        document_metadata.insert(
            "normalized_byte_len".to_string(),
            serde_json::json!(bounds.byte_len),
        );
        document_metadata.insert(
            "normalized_char_count".to_string(),
            serde_json::json!(bounds.char_count),
        );
        let document = PreparedDocument {
            document_id: request.document.document_id,
            source_id: request.document.source_id,
            source_item_key: request.document.source_item_key,
            generation: request.generation,
            canonical_uri: request.document.canonical_uri,
            prepare_version: "axon-document-pr8".to_string(),
            chunking_profile: effective_profile.as_str().to_string(),
            chunking_method: effective_profile.as_str().to_string(),
            chunks: prepared_chunks,
            metadata: document_metadata,
            cleanup_keys: Vec::<CleanupKey>::new(),
            graph_refs: Vec::new(),
            parse_facts: request.parse_facts,
            graph_candidates: request.graph_candidates,
            warnings,
            errors: request.errors,
        };
        validate_prepared_document(&document)?;
        Ok(PrepareSourceDocumentResult { document })
    }
}

/// Fold parser-produced facts/candidates/diagnostics into the request so they
/// reach the `PreparedDocument`. No-op when the caller already supplied facts
/// (in which case `parse` is the default, empty value).
fn merge_parse_artifacts(request: &mut PrepareSourceDocumentRequest, parse: &DocumentParse) {
    if !parse.parse_facts.is_empty() {
        request.parse_facts = parse.parse_facts.clone();
    }
    if !parse.graph_candidates.is_empty() {
        request.graph_candidates = parse.graph_candidates.clone();
    }
    request.warnings.extend(parse.warnings.iter().cloned());
    request.errors.extend(parse.errors.iter().cloned());
}

struct PreparedContentText {
    text: String,
    warnings: Vec<SourceWarning>,
    force_profile: Option<ChunkingProfile>,
}

fn content_text(document: &axon_api::source::SourceDocument) -> PreparedContentText {
    match &document.content {
        ContentRef::InlineText { text } => PreparedContentText {
            text: text.clone(),
            warnings: Vec::new(),
            force_profile: None,
        },
        ContentRef::InlineBytes {
            bytes_base64,
            mime_type,
        } => PreparedContentText {
            text: format!(
                "inline bytes omitted from text preparation\nmime_type: {mime_type}\nencoded_bytes: {}",
                bytes_base64.len()
            ),
            warnings: vec![warning(
                "document.content.inline_bytes_fallback",
                "inline bytes prepared as bounded metadata text",
                &document.source_item_key,
            )],
            force_profile: Some(ChunkingProfile::AtomicMetadata),
        },
        ContentRef::Artifact { artifact_id } => PreparedContentText {
            text: format!("artifact content reference\nartifact_id: {}", artifact_id.0),
            warnings: vec![warning(
                "document.content.artifact_fallback",
                "artifact content prepared as metadata reference",
                &document.source_item_key,
            )],
            force_profile: Some(ChunkingProfile::AtomicMetadata),
        },
        ContentRef::External { uri, integrity } => PreparedContentText {
            text: format!(
                "external content reference\nuri: {uri}\nintegrity: {}",
                integrity.as_deref().unwrap_or("unknown")
            ),
            warnings: vec![warning(
                "document.content.external_fallback",
                "external content prepared as metadata reference",
                &document.source_item_key,
            )],
            force_profile: Some(ChunkingProfile::AtomicMetadata),
        },
    }
}

struct ChunkBuild {
    chunks: Vec<DocumentChunk>,
    warnings: Vec<SourceWarning>,
}

fn build_chunks(
    profile: ChunkingProfile,
    text: &str,
    structured_payload: Option<&serde_json::Value>,
    source_item_key: &SourceItemKey,
) -> ChunkBuild {
    let chunks = match profile {
        ChunkingProfile::CodeSymbol => code::code_symbols(text),
        ChunkingProfile::CodeManifest => code::code_manifest(text),
        ChunkingProfile::MarkdownSections => markdown::markdown_sections(text),
        ChunkingProfile::HtmlArticle => markdown::html_article(text),
        ChunkingProfile::PlainTextWindows => text::plain_text_windows(text),
        ChunkingProfile::TranscriptSegments => transcript::transcript_segments(text),
        ChunkingProfile::StructuredRecords => {
            return structured_or_fallback(
                profile,
                metadata::structured_records(text, structured_payload),
                text,
                source_item_key,
            );
        }
        ChunkingProfile::ApiSchema => {
            return structured_or_fallback(
                profile,
                schema::api_schema(text, structured_payload),
                text,
                source_item_key,
            );
        }
        ChunkingProfile::ToolOutput => transcript::split_on_nonempty_lines(text, "tool_output"),
        ChunkingProfile::SessionTurns => session::session_turns(text),
        ChunkingProfile::AtomicMetadata => metadata::atomic_metadata(text),
    };
    ChunkBuild {
        chunks,
        warnings: Vec::new(),
    }
}

fn structured_or_fallback(
    profile: ChunkingProfile,
    result: Result<Vec<DocumentChunk>, String>,
    text: &str,
    source_item_key: &SourceItemKey,
) -> ChunkBuild {
    match result {
        Ok(chunks) => ChunkBuild {
            chunks,
            warnings: Vec::new(),
        },
        Err(error) => ChunkBuild {
            chunks: metadata::atomic_metadata(text)
                .into_iter()
                .map(|chunk| {
                    chunk
                        .with_metadata("chunking_fallback", "atomic_text".into())
                        .with_metadata("chunking_fallback_from", profile.as_str().into())
                        .with_metadata("structured_parse_error", error.clone().into())
                })
                .collect(),
            warnings: vec![warning(
                "chunk.structured_parse_failed",
                format!(
                    "structured chunk parse failed for {}: {error}",
                    profile.as_str()
                ),
                source_item_key,
            )],
        },
    }
}

fn warning(
    code: impl Into<String>,
    message: impl Into<String>,
    source_item_key: &SourceItemKey,
) -> SourceWarning {
    SourceWarning {
        code: code.into(),
        severity: Severity::Warning,
        message: message.into(),
        source_item_key: Some(source_item_key.clone()),
        retryable: false,
    }
}

fn prepare_chunks(
    request: &PrepareSourceDocumentRequest,
    profile: ChunkingProfile,
    chunks: Vec<DocumentChunk>,
    parser_stamp: Option<(&str, &str)>,
) -> Vec<PreparedChunk> {
    let len = chunks.len();
    let mut prepared: Vec<PreparedChunk> = chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| build_prepared_chunk(request, profile, parser_stamp, idx, chunk))
        .collect();

    for idx in 0..len {
        if idx > 0 {
            prepared[idx].previous_chunk_id = Some(prepared[idx - 1].chunk_id.clone());
        }
        if idx + 1 < len {
            prepared[idx].next_chunk_id = Some(prepared[idx + 1].chunk_id.clone());
        }
    }

    prepared
}

fn build_prepared_chunk(
    request: &PrepareSourceDocumentRequest,
    profile: ChunkingProfile,
    parser_stamp: Option<(&str, &str)>,
    idx: usize,
    mut chunk: DocumentChunk,
) -> PreparedChunk {
    chunk
        .metadata
        .insert("chunking_profile".to_string(), profile.as_str().into());
    if let Some((parser_id, parser_version)) = parser_stamp {
        chunk
            .metadata
            .insert("parser_id".to_string(), parser_id.into());
        chunk
            .metadata
            .insert("parser_version".to_string(), parser_version.into());
    }
    let path = chunk
        .metadata
        .get("original_path")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| request.document.path.clone());
    let content_hash = simple_hash(&chunk.content);
    let chunk_key = stable_chunk_key(
        request,
        profile,
        idx,
        path.as_deref(),
        &chunk,
        &content_hash,
    );
    let chunk_id = ChunkId::from(format!("chunk_{}", stable_token(&chunk_key)));
    let symbol = chunk.symbol.clone();
    let mut metadata = merge_metadata(&request.document.metadata, chunk.metadata);
    if profile == ChunkingProfile::CodeSymbol
        && let Some(symbol) = &symbol
    {
        metadata.insert("code_symbol_name".to_string(), symbol.clone().into());
        metadata.insert(
            "code_symbol_kind".to_string(),
            code_symbol_kind(&chunk.content).into(),
        );
    }
    PreparedChunk {
        chunk_id,
        chunk_key,
        document_id: request.document.document_id.clone(),
        chunk_index: idx as u32,
        content_hash,
        embedding_text: None,
        chunk_locator: ChunkLocator {
            canonical_uri: request.document.canonical_uri.clone(),
            path,
            heading_path: chunk.heading_path,
            symbol,
            range: chunk.range.clone(),
        },
        source_range: chunk.range,
        content_kind: request.document.content_kind,
        title: chunk.title.or_else(|| request.document.title.clone()),
        graph_refs: Vec::new(),
        parent_chunk_id: None,
        previous_chunk_id: None,
        next_chunk_id: None,
        metadata,
        content: chunk.content,
    }
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
    if let Err(error) = validate_prepared_document_ranges(document) {
        errors.push(error);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

pub(crate) fn validate_prepared_document_ranges(document: &PreparedDocument) -> Result<(), String> {
    let Some(bounds) = document_bounds(document) else {
        return Ok(());
    };
    for chunk in &document.chunks {
        validate_source_range(&chunk.source_range, &bounds)
            .map_err(|error| format!("chunk {} source_range {error}", chunk.chunk_id.0))?;
        validate_source_range(&chunk.chunk_locator.range, &bounds)
            .map_err(|error| format!("chunk {} locator range {error}", chunk.chunk_id.0))?;
    }
    for fact in &document.parse_facts {
        if let Some(range) = &fact.range {
            validate_source_range(range, &bounds)
                .map_err(|error| format!("parse fact {} range {error}", fact.name))?;
        }
    }
    Ok(())
}

fn document_bounds(document: &PreparedDocument) -> Option<SourceRangeBounds> {
    Some(SourceRangeBounds {
        line_count: document.metadata.get("normalized_line_count")?.as_u64()? as u32,
        byte_len: document.metadata.get("normalized_byte_len")?.as_u64()?,
        char_count: document.metadata.get("normalized_char_count")?.as_u64()?,
    })
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

fn code_symbol_kind(content: &str) -> &'static str {
    let first = content.lines().next().unwrap_or_default().trim_start();
    if first.starts_with("pub fn ") || first.starts_with("fn ") || first.starts_with("def ") {
        "function"
    } else if first.starts_with("class ") || first.starts_with("struct ") {
        "type"
    } else if first.starts_with("enum ") {
        "enum"
    } else if first.starts_with("impl ") {
        "impl"
    } else {
        "symbol"
    }
}

fn simple_hash(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn stable_token(text: &str) -> String {
    simple_hash(text).trim_start_matches("fnv1a64:").to_string()
}

fn stable_chunk_key(
    request: &PrepareSourceDocumentRequest,
    profile: ChunkingProfile,
    idx: usize,
    path: Option<&str>,
    chunk: &DocumentChunk,
    content_hash: &str,
) -> String {
    let range = &chunk.range;
    let locator = format!(
        "path={}|heading={}|symbol={}|line={:?}-{:?}|byte={:?}-{:?}|char={:?}-{:?}|json={:?}|yaml={:?}|session={:?}",
        path.unwrap_or(""),
        chunk.heading_path.join("/"),
        chunk.symbol.as_deref().unwrap_or(""),
        range.line_start,
        range.line_end,
        range.byte_start,
        range.byte_end,
        range.char_start,
        range.char_end,
        range.json_pointer,
        range.yaml_path,
        range.session_turn_id,
    );
    let raw_key = format!(
        "source={}|generation={}|item={}|document={}|profile={}|index={idx}|{locator}|content={content_hash}",
        request.document.source_id.0,
        request.generation.0,
        request.document.source_item_key.0,
        request.document.document_id.0,
        profile.as_str(),
    );
    format!(
        "{}:{}:{}:{}:{}",
        request.document.source_id.0,
        request.generation.0,
        request.document.source_item_key.0,
        profile.as_str(),
        stable_token(&raw_key)
    )
}

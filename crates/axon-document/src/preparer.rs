//! Source document preparation entry point.

use axon_api::source::{
    ChunkId, ChunkLocator, CleanupKey, ContentRef, MetadataMap, PreparedChunk, PreparedDocument,
    SourceDocument, SourceItemKey, SourceWarning,
};
use axon_parse::vertical::take_metadata_artifacts;

use crate::chunk::DocumentChunk;
use crate::chunk_router::{ChunkRouter, decision_for_profile, source_adapter, source_scope};
use crate::parse::{DocumentParse, parse_document};
use crate::prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use crate::profile::ChunkingProfile;
use crate::source_range::bounds_for_text;

mod chunk_build;
mod validation;
use chunk_build::{build_chunks, warning};
#[cfg(test)]
pub(crate) use validation::validate_prepared_document;
#[cfg(test)]
pub(crate) use validation::validate_prepared_document_ranges_against_bounds;
use validation::validate_prepared_document_with_bounds;

#[derive(Debug, Default, Clone)]
pub struct DocumentPreparer {
    router: ChunkRouter,
}

impl DocumentPreparer {
    pub fn prepare(
        &self,
        mut request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        // Dead-code recovery (#298 alignment): mirror any off-band
        // structured-data extraction into ordinary metadata *before* routing/
        // chunking, so it survives into the vector payload instead of being
        // silently dropped for markdown-routed web documents. See the
        // function doc comment for the full rationale.
        project_structured_payload_metadata(&mut request.document);
        let metadata_parse = take_metadata_artifacts(&mut request.document.metadata);
        // Pass 1 of 2: pre-chunk redaction. `axon-vectors` runs the second
        // (final, authoritative) pass at vector-payload build time; scrubbing
        // here as well keeps secrets out of the chunk boundaries, chunk
        // hashes, and any parse facts/graph candidates derived from the text,
        // not just the eventual payload.
        //
        // Redaction MUST precede the self-parse below: parse facts and graph
        // candidates carry line numbers and quotes, and range/quote validation
        // later slices the redacted text. A redaction pass that rewrites the
        // content between parsing and validation would shift or alter lines
        // and fail preparation with "quote outside source range" (seen live
        // with fenced `Authorization: Bearer …` examples in docs).
        let content = redact_pre_chunk(
            content_text(&request.document),
            &request.document.source_item_key,
        );
        // Activate axon-parse on the acquisition path: when the caller did not
        // pre-supply parse facts, parse the document here so parser-driven chunk
        // routing and graph candidates actually flow. Callers that already carry
        // facts (or explicitly forced a profile) keep their supplied artifacts.
        // The self-parse sees the same post-redaction text that chunking and
        // range validation use.
        let parse = if request.parse_facts.is_empty() && metadata_parse.facts.is_empty() {
            let mut parse_doc = request.document.clone();
            parse_doc.content = ContentRef::InlineText {
                text: content.text.clone(),
            };
            parse_document(&parse_doc)
        } else {
            DocumentParse::default()
        };
        merge_parse_artifacts(&mut request, &parse);
        request.parse_facts.extend(metadata_parse.facts);
        request
            .graph_candidates
            .extend(metadata_parse.graph_candidates);

        let profile = match request.profile {
            Some(profile) => profile,
            None => parse
                .routed_profile()
                .map(Ok)
                .unwrap_or_else(|| self.router.route(&request.document))?,
        };
        let bounds = bounds_for_text(&content.text);
        let effective_profile = content.force_profile.unwrap_or(profile);
        // Concrete method distinct from the profile name: routes through the
        // same size/adapter/scope-aware decision `ChunkRouter::route_decision`
        // uses (adapter/scope read from the same shared metadata envelope),
        // keyed off the *effective* profile (which may differ from the
        // router's raw pick when the content ref forces atomic metadata) and
        // the post-redaction content length actually handed to the chunker.
        let decision = decision_for_profile(
            effective_profile,
            content.text.len(),
            source_adapter(&request.document),
            source_scope(&request.document),
        );
        // The decision's method is only reported truthfully when the actual
        // chunk-building dispatch below honors it: a size/adapter-triggered
        // fallback swaps the structural chunker for a generic windowed split
        // so `chunking_method` never claims a method that did not run.
        let use_size_or_adapter_fallback = decision.method != decision.fallback_chain[0];
        let build = build_chunks(
            effective_profile,
            &content.text,
            request.document.structured_payload.as_ref(),
            &request.document.source_item_key,
            request.document.path.as_deref(),
            request.document.language.as_deref(),
            request.document.content_kind,
            &request.parse_facts,
            use_size_or_adapter_fallback,
        );
        let chunks = build.chunks;
        let parsed_code_method = (effective_profile == ChunkingProfile::CodeSymbol
            && !use_size_or_adapter_fallback)
            .then(|| {
                chunks.first().and_then(|chunk| {
                    chunk
                        .metadata
                        .get("actual_chunking_method")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
            })
            .flatten();
        let parser_stamp = (!parse.parser_id.is_empty() && parse.parser_id != "none")
            .then_some((parse.parser_id.as_str(), parse.parser_version.as_str()));
        let chunking_method = if content.force_profile.is_some() {
            "atomic_metadata"
        } else if !build.warnings.is_empty() {
            // `structured_or_fallback` degraded to atomic text.
            "atomic_fallback"
        } else if let Some(method) = parsed_code_method.as_deref() {
            method
        } else {
            decision.method
        };
        let prepared_chunks = prepare_chunks(&request, effective_profile, chunks, parser_stamp);
        let mut warnings = request.warnings;
        warnings.extend(content.warnings);
        warnings.extend(build.warnings);
        let document_metadata = request.document.metadata;
        let document = PreparedDocument {
            document_id: request.document.document_id,
            source_id: request.document.source_id,
            source_item_key: request.document.source_item_key,
            generation: request.generation,
            canonical_uri: request.document.canonical_uri,
            prepare_version: "axon-document-pr8".to_string(),
            chunking_profile: effective_profile.as_str().to_string(),
            chunking_method: chunking_method.to_string(),
            chunks: prepared_chunks,
            metadata: document_metadata,
            cleanup_keys: Vec::<CleanupKey>::new(),
            graph_refs: Vec::new(),
            parse_facts: request.parse_facts,
            graph_candidates: request.graph_candidates,
            warnings,
            errors: request.errors,
        };
        validate_prepared_document_with_bounds(&document, &bounds, &content.text)?;
        Ok(PrepareSourceDocumentResult { document })
    }
}

/// Bound the size of a structured-data blob attached to document/chunk
/// metadata. Mirrors the web source adapter's own
/// `bounded_structured_payload` cap (`crates/axon-adapters/src/web.rs`, 64
/// KiB) as a consumer-side safety net: `SourceDocument::structured_payload`
/// can in principle be populated by any caller (see
/// `axon-services::scrape::scrape_result_to_prepared_doc`), not only the
/// crawl-manifest path that already enforces the cap.
const MAX_STRUCTURED_METADATA_BYTES: usize = 64 * 1024;

/// Project a web document's off-band structured-data extraction (JSON-LD /
/// `__NEXT_DATA__` / SvelteKit island, captured on
/// `SourceDocument::structured_payload` -- see
/// `axon-adapters::web::bounded_structured_payload` and
/// `axon-crawl::engine::collector::page::extract_structured_blob` for how it
/// is populated) into ordinary document metadata, so it survives into the
/// vector payload instead of being silently dropped.
///
/// `structured_payload` is a dedicated `SourceDocument` field, not a
/// `MetadataMap` entry. Profile-specific chunk builders only ever see it via
/// `build_chunks`'s explicit `structured_payload` parameter
/// (`preparer::chunk_build`), which is consumed solely by the
/// `StructuredRecords`/`ApiSchema` profiles (`metadata::structured_records`,
/// `schema::api_schema`). Every other profile -- including
/// `MarkdownSections`, the profile virtually every web/crawl document routes
/// to (`ContentKind::Markdown`) -- never touches it, so without this
/// projection the value never reaches a chunk or a Qdrant payload at all.
///
/// Instead of threading a new parameter through every chunk builder, this
/// mirrors the payload into `document.metadata` *before* chunking, reusing
/// two mechanisms that already exist:
/// - `build_prepared_chunk`'s `merge_metadata` call below copies any
///   document-level metadata key down onto every chunk's own metadata (this
///   is how `web_title`/`web_domain` already reach every chunk).
/// - `axon-vectors::point::point_payload::build_payload` starts each vector
///   point's payload from `document.metadata.clone()` before layering
///   per-chunk metadata on top, so the projected fields land in the Qdrant
///   payload for every chunk of the document regardless of profile.
///
/// Gated to the `web` source family: `web_structured_kind`/
/// `web_structured_blob` are declared only in that family's vector-payload
/// allowlist (`axon_vectors::payload_families::VECTOR_SOURCE_FAMILY_FIELDS`),
/// so projecting them for any other family would fail payload validation
/// with `UnknownSourceSpecificField`.
fn project_structured_payload_metadata(document: &mut SourceDocument) {
    let is_web = document
        .metadata
        .get("source_family")
        .and_then(serde_json::Value::as_str)
        == Some("web");
    if !is_web {
        return;
    }
    let Some(payload) = document.structured_payload.as_ref() else {
        return;
    };
    let Ok(blob) = serde_json::to_string(payload) else {
        return;
    };
    if blob.len() > MAX_STRUCTURED_METADATA_BYTES {
        return;
    }
    if let Some(kind) = structured_kind_label(payload) {
        document
            .metadata
            .insert("web_structured_kind".to_string(), kind.into());
    }
    document
        .metadata
        .insert("web_structured_blob".to_string(), blob.into());
}

/// Best available short label for the structured payload's schema identity:
/// the schema.org/JSON-LD type (the crawl-manifest envelope's `schema_type`
/// field, resolved by `axon_core::structured::schema_type_of`) when present,
/// falling back to the coarser extraction mechanism recorded under `kind`
/// (`jsonld`, `next_data`, or `sveltekit`, per `StructuredDataPass::dominant`).
fn structured_kind_label(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("schema_type")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("kind").and_then(serde_json::Value::as_str))
        .map(str::to_string)
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

/// Redaction pass 1 of 2: scrubs sensitive values out of the normalized
/// content *before* chunking, so chunk boundaries/hashes/derived parse facts
/// never see raw secrets. Pass 2 (authoritative, fail-closed) runs at
/// vector-payload build time in `axon-vectors`.
fn redact_pre_chunk(
    content: PreparedContentText,
    source_item_key: &SourceItemKey,
) -> PreparedContentText {
    // Span-level scrub: replace each secret-shaped run with the redaction
    // placeholder while keeping the rest of the document intact. The
    // tombstone-style `Redactor::redact_text` is for short free-text
    // surfaces (log lines, transport messages) and replaces the ENTIRE
    // input when any secret is present — applied to a whole document it
    // turned one fenced `Authorization: Bearer …` example into a
    // single-line "[REDACTED]" body, destroying the document and every
    // parse fact derived from it.
    let redacted = axon_core::redact::redact_secrets(&content.text);
    if redacted == content.text {
        return content;
    }
    let mut warnings = content.warnings;
    warnings.push(warning(
        "document.content.pre_chunk_redacted",
        "pre-chunk redaction pass scrubbed sensitive values before chunking",
        source_item_key,
    ));
    PreparedContentText {
        text: redacted,
        warnings,
        force_profile: content.force_profile,
    }
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
        metadata
            .entry("code_symbol_kind".to_string())
            .or_insert_with(|| crate::code::code_symbol_kind_for_content(&chunk.content).into());
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

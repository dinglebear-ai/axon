//! Pure helpers for [`super`]'s vector-point batch construction: deterministic
//! point ids, chunk hashing, locator/range JSON serialization, the metadata
//! redaction pass, and small metadata utilities. Kept out of `point.rs` so the
//! module root stays under the monolith cap.

use axon_api::source::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::redactor::{DefaultRedactor, RedactionContext, RedactionReport, redact_metadata};

/// Insert `value` for `field` only when the field is absent or blank.
pub(super) fn insert_default_string(metadata: &mut MetadataMap, field: &str, value: &str) {
    if metadata
        .get(field)
        .and_then(|existing| existing.as_str())
        .is_none_or(|existing| existing.trim().is_empty())
    {
        metadata.insert(field.to_string(), json!(value));
    }
}

/// Deterministic vector-point id stable for `(collection, vector_namespace,
/// document_id, chunk_id, source_generation)`.
pub(super) fn stable_point_id(
    collection: &str,
    vector_namespace: &str,
    document_id: &DocumentId,
    chunk_id: &ChunkId,
    source_generation: &SourceGenerationId,
) -> VectorPointId {
    let key = format!(
        "{collection}\0{vector_namespace}\0{}\0{}\0{}",
        document_id.0, chunk_id.0, source_generation.0
    );
    VectorPointId::new(Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes()).to_string())
}

/// `sha256:<hex>` over the normalized chunk text plus a stable serialization of
/// the chunk locator (canonical URI, path, heading path, symbol, and source
/// range). Per the vector-payload contract, `chunk_hash` changes when either the
/// chunk text or its source range/locator changes, so both feed the digest.
pub(super) fn chunk_hash(chunk: &PreparedChunk, locator: &ChunkLocator) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chunk.content.as_bytes());
    hasher.update([0u8]);
    // A canonical (deterministic) JSON serialization of the locator — including
    // its source range — is stable across transports and stores.
    hasher.update(chunk_locator_json(locator).to_string().as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub(super) fn chunk_locator_json(locator: &ChunkLocator) -> Value {
    json!({
        "canonical_uri": locator.canonical_uri,
        "path": locator.path,
        "heading_path": locator.heading_path,
        "symbol": locator.symbol,
        "range": source_range_json(&locator.range),
    })
}

pub(super) fn source_range_json(range: &SourceRange) -> Value {
    json!({
        "line_start": range.line_start,
        "line_end": range.line_end,
        "byte_start": range.byte_start,
        "byte_end": range.byte_end,
        "char_start": range.char_start,
        "char_end": range.char_end,
        "time_start_ms": range.time_start_ms,
        "time_end_ms": range.time_end_ms,
        "dom_selector": range.dom_selector,
        "json_pointer": range.json_pointer,
        "yaml_path": range.yaml_path,
        "xml_xpath": range.xml_xpath,
        "csv_row": range.csv_row,
        "session_turn_id": range.session_turn_id,
        "turn_start": range.turn_start,
        "turn_end": range.turn_end,
    })
}

/// Run the contract Redactor over the assembled payload metadata and stamp an
/// accurate `redaction_status` (`clean` vs `redacted`) derived from the pass.
///
/// The redactor drops secret-named metadata fields and scrubs secret-shaped
/// values, but deliberately leaves `chunk_text` untouched — a genuine secret in
/// the retrievable body must still trip the `ForbiddenValue` validator and skip
/// the chunk (handled by the caller), not be laundered into the index.
pub(super) fn apply_redaction(metadata: MetadataMap, chunk: &PreparedChunk) -> MetadataMap {
    let source_kind = metadata
        .get("source_kind")
        .and_then(|value| value.as_str())
        .and_then(|value| serde_json::from_value(json!(value)).ok());
    let (mut metadata, report) = redact_metadata(
        metadata,
        &RedactionContext::vector_payload(source_kind),
        &DefaultRedactor::new(),
    );
    metadata.insert(
        "redaction_status".to_string(),
        json!(report.status().as_str()),
    );
    log_redaction(chunk, &report);
    metadata
}

fn log_redaction(chunk: &PreparedChunk, report: &RedactionReport) {
    if report.status_redacted {
        tracing::debug!(
            chunk_id = %chunk.chunk_id.0,
            redacted_fields = report.redacted_fields.len(),
            dropped_fields = report.dropped_fields.len(),
            detectors = ?report.detectors_triggered,
            "redacted vector payload metadata"
        );
    }
}

//! Vector-payload assembly for a single chunk.
//!
//! Split out of `point.rs` to keep that file under the repo's 500-line
//! monolith cap. `build_payload` is the only item the parent module needs;
//! everything else here is a private implementation detail.

use axon_api::source::*;
use serde_json::json;

use crate::payload::{VECTOR_PAYLOAD_CONTRACT_VERSION, VectorPayload, generation_payload_i64};
use crate::point::{VectorPointBatchBuildContext, VectorPointBatchBuildError};

use super::build_helpers::{
    apply_redaction, chunk_hash, chunk_locator_json, insert_default_string, source_range_json,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_payload(
    collection: &CollectionSpec,
    document: &PreparedDocument,
    chunk: &PreparedChunk,
    point_id: &VectorPointId,
    batch_id: &BatchId,
    job_id: &JobId,
    provider_id: &ProviderId,
    model: &str,
    context: &VectorPointBatchBuildContext,
) -> Result<MetadataMap, VectorPointBatchBuildError> {
    let mut metadata = document.metadata.clone();
    metadata.remove("embedding_batch_id");
    metadata.remove("embedding_provider_id");
    for (field, value) in chunk.metadata.0.clone() {
        if !PREPARER_INTERNAL_CHUNK_METADATA.contains(&field.as_str()) {
            metadata.insert(field, value);
        }
    }
    metadata.insert(
        "payload_contract_version".to_string(),
        json!(VECTOR_PAYLOAD_CONTRACT_VERSION),
    );
    metadata.insert("collection".to_string(), json!(collection.collection));
    metadata.insert("vector_point_id".to_string(), json!(point_id.0));
    metadata.insert("source_id".to_string(), json!(document.source_id.0));
    metadata.insert(
        "source_item_key".to_string(),
        json!(document.source_item_key.0),
    );
    // `source_canonical_uri` is the canonical URI of the source *identity*,
    // distinct from `item_canonical_uri` (the item/page/file). Adapters that
    // resolve a distinct source identity stamp it into document metadata; when
    // absent (single-item sources), it collapses onto the item canonical URI.
    let source_canonical_uri = metadata
        .get("source_canonical_uri")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| document.canonical_uri.clone());
    metadata.insert(
        "source_canonical_uri".to_string(),
        json!(source_canonical_uri),
    );
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(document.canonical_uri),
    );
    metadata.insert(
        "source_generation".to_string(),
        json!(
            generation_payload_i64(&document.generation, "source_generation").map_err(
                |source| VectorPointBatchBuildError::Payload {
                    chunk_id: chunk.chunk_id.clone(),
                    source,
                }
            )?
        ),
    );
    metadata.insert("committed_generation".to_string(), serde_json::Value::Null);
    metadata.insert("document_id".to_string(), json!(document.document_id.0));
    metadata.insert("chunk_id".to_string(), json!(chunk.chunk_id.0));
    metadata.insert("chunk_key".to_string(), json!(chunk.chunk_key));
    metadata.insert("chunk_index".to_string(), json!(chunk.chunk_index));
    // Distinct from `embedding_profile` below (S2-27/S2-18): the chunking
    // profile/method the preparer selected, stripped out of
    // `chunk.metadata` above by `PREPARER_INTERNAL_CHUNK_METADATA` and
    // re-added here from the authoritative `PreparedDocument` fields.
    metadata.insert(
        "chunking_profile".to_string(),
        json!(document.chunking_profile),
    );
    metadata.insert(
        "chunking_method".to_string(),
        json!(document.chunking_method),
    );
    metadata.insert("content_hash".to_string(), json!(chunk.content_hash));
    metadata.insert(
        "chunk_hash".to_string(),
        json!(chunk_hash(chunk, &chunk.chunk_locator)),
    );
    metadata.insert("chunk_text".to_string(), json!(chunk.content));
    metadata.insert("content_kind".to_string(), json!(chunk.content_kind));
    metadata.insert(
        "chunk_locator".to_string(),
        chunk_locator_json(&chunk.chunk_locator),
    );
    metadata.insert(
        "source_range".to_string(),
        source_range_json(&chunk.source_range),
    );
    insert_default_string(&mut metadata, "visibility", "internal");
    metadata.insert("job_id".to_string(), json!(job_id.0.to_string()));
    metadata.insert(
        "embedding_batch_id".to_string(),
        json!(batch_id.0.to_string()),
    );
    metadata.insert("document_status".to_string(), json!("vectorized"));
    metadata.insert("embedding_model".to_string(), json!(model));
    metadata.insert(
        "embedding_dimensions".to_string(),
        json!(collection.dense.dimensions),
    );
    metadata.insert(
        "embedding_provider".to_string(),
        json!(provider_id.0.clone()),
    );
    // `embedding_profile` identifies the embedding-pipeline profile (today
    // always full-document embedding, distinct from a hypothetical future
    // "query" embedding profile) -- it must never be repurposed to carry the
    // chunking profile (see `chunking_profile` above).
    metadata.insert(
        "embedding_profile".to_string(),
        json!(DEFAULT_EMBEDDING_PROFILE),
    );
    metadata.insert(
        "embedded_at".to_string(),
        json!(context.embedded_at.0.clone()),
    );
    metadata.insert("vector_namespace".to_string(), json!(collection.dense.name));

    // Run the contract Redactor over the assembled metadata and stamp an
    // accurate `redaction_status` (`clean` vs `redacted`) BEFORE validation.
    let metadata = apply_redaction(metadata, chunk);

    VectorPayload::try_from_metadata(metadata)
        .map(|payload| payload.into_metadata())
        .map_err(|source| VectorPointBatchBuildError::Payload {
            chunk_id: chunk.chunk_id.clone(),
            source,
        })
}

/// Only embedding-pipeline profile in use today: every vector point is a
/// full-document chunk embedding. A future asymmetric-embedding "query"
/// profile would add a second constant here, not repurpose this one.
const DEFAULT_EMBEDDING_PROFILE: &str = "document";

const PREPARER_INTERNAL_CHUNK_METADATA: &[&str] = &[
    "chunking_profile",
    "chunking_method",
    "preparer_version",
    // Parser provenance stamped by axon-document's parse bridge. Kept as a
    // preparer-internal diagnostic (like `chunking_profile`) rather than a
    // strict vector-payload field, so it does not expand the payload contract.
    "parser_id",
    "parser_version",
];

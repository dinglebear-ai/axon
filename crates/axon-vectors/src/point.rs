//! Vector point batch construction.

use std::collections::BTreeSet;
use std::fmt;

use axon_api::source::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::payload::{
    VECTOR_PAYLOAD_CONTRACT_VERSION, VectorPayload, VectorPayloadValidationError,
};

pub const MODULE_NAME: &str = "point";

#[derive(Debug, Clone)]
pub struct VectorPointBatchBuilder {
    collection: CollectionSpec,
    document: PreparedDocument,
    embeddings: EmbeddingResult,
    context: VectorPointBatchBuildContext,
}

#[derive(Debug, Clone)]
pub struct VectorPointBatchBuildContext {
    pub embedded_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorPointBatchBuildError {
    DuplicateChunkId {
        chunk_id: ChunkId,
    },
    UnexpectedEmbeddingChunk {
        chunk_id: ChunkId,
    },
    MissingEmbeddingChunk {
        chunk_id: ChunkId,
    },
    DimensionMismatch {
        chunk_id: Option<ChunkId>,
        expected: u32,
        actual: u32,
    },
    InvalidDenseVector {
        chunk_id: ChunkId,
    },
    EmbeddingBatchMismatch {
        expected: BatchId,
        actual: BatchId,
    },
    InvalidEmbeddingBatchId {
        value: String,
    },
    EmbeddingProviderMismatch {
        expected: ProviderId,
        actual: ProviderId,
    },
    EmbeddingModelMismatch {
        expected: String,
        actual: String,
    },
    Payload {
        chunk_id: ChunkId,
        source: VectorPayloadValidationError,
    },
}

impl fmt::Display for VectorPointBatchBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateChunkId { chunk_id } => {
                write!(f, "duplicate vector chunk id `{}`", chunk_id.0)
            }
            Self::UnexpectedEmbeddingChunk { chunk_id } => {
                write!(f, "embedding returned unexpected chunk id `{}`", chunk_id.0)
            }
            Self::MissingEmbeddingChunk { chunk_id } => {
                write!(f, "missing embedding for chunk id `{}`", chunk_id.0)
            }
            Self::DimensionMismatch {
                chunk_id,
                expected,
                actual,
            } => {
                if let Some(chunk_id) = chunk_id {
                    write!(
                        f,
                        "chunk `{}` has {actual} embedding dimensions, expected {expected}",
                        chunk_id.0
                    )
                } else {
                    write!(
                        f,
                        "embedding result declares {actual} dimensions, expected {expected}"
                    )
                }
            }
            Self::EmbeddingBatchMismatch { expected, actual } => {
                write!(
                    f,
                    "embedding result batch `{}` does not match embedding batch `{}`",
                    actual.0, expected.0
                )
            }
            Self::InvalidEmbeddingBatchId { value } => {
                write!(f, "embedding batch id `{value}` is not a valid UUID")
            }
            Self::InvalidDenseVector { chunk_id } => {
                write!(
                    f,
                    "embedding vector for chunk `{}` contains non-finite values",
                    chunk_id.0
                )
            }
            Self::EmbeddingProviderMismatch { expected, actual } => {
                write!(
                    f,
                    "embedding result provider `{}` does not match embedding batch provider `{}`",
                    actual.0, expected.0
                )
            }
            Self::EmbeddingModelMismatch { expected, actual } => {
                write!(
                    f,
                    "embedding result model `{actual}` does not match embedding batch model `{expected}`"
                )
            }
            Self::Payload { chunk_id, source } => {
                write!(
                    f,
                    "invalid vector payload for chunk `{}`: {source}",
                    chunk_id.0
                )
            }
        }
    }
}

impl std::error::Error for VectorPointBatchBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Payload { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl VectorPointBatchBuilder {
    pub fn new(
        collection: CollectionSpec,
        document: PreparedDocument,
        embeddings: EmbeddingResult,
        context: VectorPointBatchBuildContext,
    ) -> Self {
        Self {
            collection,
            document,
            embeddings,
            context,
        }
    }

    pub fn build(self) -> Result<VectorPointBatch, VectorPointBatchBuildError> {
        let expected_dimensions = self.collection.dense.dimensions;
        if self.embeddings.dimensions != expected_dimensions {
            return Err(VectorPointBatchBuildError::DimensionMismatch {
                chunk_id: None,
                expected: expected_dimensions,
                actual: self.embeddings.dimensions,
            });
        }

        validate_embedding_provenance(&self.document, &self.embeddings)?;
        let chunks = chunks_by_id(&self.document)?;
        let batch_id = self.embeddings.batch_id.clone();
        let job_id = self.embeddings.job_id.clone();
        let provider_id = self.embeddings.provider_id.clone();
        let model = self.embeddings.model.clone();
        let mut vectors =
            vectors_by_chunk_id(self.embeddings.vectors, &chunks, expected_dimensions)?;
        let mut points = Vec::with_capacity(self.document.chunks.len());

        for chunk in &self.document.chunks {
            let vector = vectors.remove(&chunk.chunk_id).ok_or_else(|| {
                VectorPointBatchBuildError::MissingEmbeddingChunk {
                    chunk_id: chunk.chunk_id.clone(),
                }
            })?;
            let point_id = stable_point_id(
                &self.collection.collection,
                &self.collection.dense.name,
                &self.document.document_id,
                &chunk.chunk_id,
                &self.document.generation,
            );
            let payload = match build_payload(
                &self.collection,
                &self.document,
                chunk,
                &point_id,
                &batch_id,
                &job_id,
                &provider_id,
                &model,
                &self.context,
            ) {
                Ok(payload) => payload,
                // Secret-redaction rejection is a per-chunk concern, not fatal:
                // skip the secret-bearing chunk (do NOT index secrets, per the
                // redaction contract) and continue, rather than aborting the
                // whole source. Arbitrary-content sources (reddit posts, AI
                // session transcripts, crawled pages) legitimately contain
                // dotenv-style lines or token-shaped strings; one such chunk
                // must not fail the entire index. Every other payload validation
                // error is a real defect and still propagates.
                Err(VectorPointBatchBuildError::Payload { chunk_id, source })
                    if matches!(
                        source,
                        crate::payload::VectorPayloadValidationError::ForbiddenValue { .. }
                    ) =>
                {
                    tracing::warn!(
                        chunk_id = %chunk_id.0,
                        "skipping chunk with secret-redaction-forbidden payload value (not indexed)"
                    );
                    continue;
                }
                Err(err) => return Err(err),
            };
            // Compute the bm42 sparse vector for hybrid (dense + sparse RRF)
            // retrieval. An all-stopword/tiny chunk yields no indexable terms →
            // a dense-only point (None), which hybrid RRF tolerates. Buckets are
            // FNV-1a-stable and must match the query-side computation.
            let sparse = crate::bm42::compute_bm42_sparse(chunk.chunk_id.clone(), &chunk.content);
            let sparse_vector = (!sparse.indices.is_empty()).then_some(sparse);
            points.push(VectorPoint {
                point_id,
                chunk_id: chunk.chunk_id.clone(),
                vector: vector.values,
                sparse_vector,
                payload,
            });
        }

        Ok(VectorPointBatch {
            batch_id,
            collection: self.collection.collection,
            points,
            model,
            dimensions: expected_dimensions,
            sparse_vectors: None,
            payload_indexes: self.collection.payload_indexes,
        })
    }
}

fn validate_embedding_provenance(
    document: &PreparedDocument,
    embeddings: &EmbeddingResult,
) -> Result<(), VectorPointBatchBuildError> {
    if let Some(batch_id) = document
        .metadata
        .get("embedding_batch_id")
        .and_then(|value| value.as_str())
        .map(parse_embedding_batch_id)
        .transpose()?
        && embeddings.batch_id != batch_id
    {
        return Err(VectorPointBatchBuildError::EmbeddingBatchMismatch {
            expected: batch_id,
            actual: embeddings.batch_id.clone(),
        });
    }
    if let Some(provider_id) = document
        .metadata
        .get("embedding_provider_id")
        .and_then(|value| value.as_str())
        .map(ProviderId::new)
        && embeddings.provider_id != provider_id
    {
        return Err(VectorPointBatchBuildError::EmbeddingProviderMismatch {
            expected: provider_id,
            actual: embeddings.provider_id.clone(),
        });
    }
    if let Some(model) = document
        .metadata
        .get("embedding_model")
        .and_then(|value| value.as_str())
        && embeddings.model != model
    {
        return Err(VectorPointBatchBuildError::EmbeddingModelMismatch {
            expected: model.to_string(),
            actual: embeddings.model.clone(),
        });
    }
    Ok(())
}

fn parse_embedding_batch_id(value: &str) -> Result<BatchId, VectorPointBatchBuildError> {
    Uuid::parse_str(value).map(BatchId::new).map_err(|_| {
        VectorPointBatchBuildError::InvalidEmbeddingBatchId {
            value: value.to_string(),
        }
    })
}

fn chunks_by_id(
    document: &PreparedDocument,
) -> Result<BTreeSet<ChunkId>, VectorPointBatchBuildError> {
    let mut ids = BTreeSet::new();
    for chunk in &document.chunks {
        if !ids.insert(chunk.chunk_id.clone()) {
            return Err(VectorPointBatchBuildError::DuplicateChunkId {
                chunk_id: chunk.chunk_id.clone(),
            });
        }
    }
    Ok(ids)
}

fn vectors_by_chunk_id(
    vectors: Vec<EmbeddingVector>,
    chunks: &BTreeSet<ChunkId>,
    expected_dimensions: u32,
) -> Result<std::collections::BTreeMap<ChunkId, EmbeddingVector>, VectorPointBatchBuildError> {
    let mut indexed = std::collections::BTreeMap::new();
    for vector in vectors {
        if vector.values.len() as u32 != expected_dimensions {
            return Err(VectorPointBatchBuildError::DimensionMismatch {
                chunk_id: Some(vector.chunk_id.clone()),
                expected: expected_dimensions,
                actual: vector.values.len() as u32,
            });
        }
        if vector.values.iter().any(|value| !value.is_finite()) {
            return Err(VectorPointBatchBuildError::InvalidDenseVector {
                chunk_id: vector.chunk_id.clone(),
            });
        }
        if !chunks.contains(&vector.chunk_id) {
            return Err(VectorPointBatchBuildError::UnexpectedEmbeddingChunk {
                chunk_id: vector.chunk_id.clone(),
            });
        }
        let chunk_id = vector.chunk_id.clone();
        if indexed.insert(chunk_id.clone(), vector).is_some() {
            return Err(VectorPointBatchBuildError::DuplicateChunkId { chunk_id });
        }
    }
    Ok(indexed)
}

#[allow(clippy::too_many_arguments)]
fn build_payload(
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
        json!(document.generation.0),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("document_id".to_string(), json!(document.document_id.0));
    metadata.insert("chunk_id".to_string(), json!(chunk.chunk_id.0));
    metadata.insert("chunk_key".to_string(), json!(chunk.chunk_key));
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
    insert_default_string(&mut metadata, "redaction_status", "clean");
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
    metadata.insert(
        "embedding_profile".to_string(),
        json!(document.chunking_profile),
    );
    metadata.insert(
        "embedded_at".to_string(),
        json!(context.embedded_at.0.clone()),
    );
    metadata.insert("vector_namespace".to_string(), json!(collection.dense.name));

    VectorPayload::try_from_metadata(metadata)
        .map(|payload| payload.into_metadata())
        .map_err(|source| VectorPointBatchBuildError::Payload {
            chunk_id: chunk.chunk_id.clone(),
            source,
        })
}

fn insert_default_string(metadata: &mut MetadataMap, field: &str, value: &str) {
    if !metadata
        .get(field)
        .and_then(|existing| existing.as_str())
        .is_some_and(|existing| !existing.trim().is_empty())
    {
        metadata.insert(field.to_string(), json!(value));
    }
}

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

fn stable_point_id(
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
fn chunk_hash(chunk: &PreparedChunk, locator: &ChunkLocator) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chunk.content.as_bytes());
    hasher.update([0u8]);
    // A canonical (deterministic) JSON serialization of the locator — including
    // its source range — is stable across transports and stores.
    hasher.update(chunk_locator_json(locator).to_string().as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn chunk_locator_json(locator: &ChunkLocator) -> Value {
    json!({
        "canonical_uri": locator.canonical_uri,
        "path": locator.path,
        "heading_path": locator.heading_path,
        "symbol": locator.symbol,
        "range": source_range_json(&locator.range),
    })
}

fn source_range_json(range: &SourceRange) -> Value {
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

//! Vector point batch construction.

mod build_helpers;
mod point_payload;

use std::collections::BTreeSet;
use std::fmt;

use axon_api::source::*;
use uuid::Uuid;

use crate::payload::VectorPayloadValidationError;
use build_helpers::stable_point_id;
use point_payload::build_payload;

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
        let (batch, _skipped_redaction) = self.build_with_skipped_count()?;
        Ok(batch)
    }

    /// Compatibility variant that returns a second count field. Redaction
    /// failures are fail-closed, so successful builds always return zero here;
    /// a forbidden payload is returned as [`VectorPointBatchBuildError::Payload`]
    /// before any partial vector batch can escape.
    pub fn build_with_skipped_count(
        self,
    ) -> Result<(VectorPointBatch, u64), VectorPointBatchBuildError> {
        let batch_id = self.embeddings.batch_id;
        let model = self.embeddings.model.clone();
        let dimensions = self.collection.dense.dimensions;
        let (points, skipped_redaction) = build_points_for_document(
            &self.collection,
            &self.document,
            self.embeddings,
            &self.context,
        )?;
        Ok((
            VectorPointBatch {
                batch_id,
                collection: self.collection.collection,
                points,
                model,
                dimensions,
                sparse_vectors: None,
                payload_indexes: self.collection.payload_indexes,
            },
            skipped_redaction,
        ))
    }
}
/// Build points for one prepared document while borrowing its chunk/payload
/// data and consuming only the embedding vectors. This avoids cloning the
/// prepared document on the source-pipeline hot path.
pub fn build_points_for_document(
    collection: &CollectionSpec,
    document: &PreparedDocument,
    embeddings: EmbeddingResult,
    context: &VectorPointBatchBuildContext,
) -> Result<(Vec<VectorPoint>, u64), VectorPointBatchBuildError> {
    let expected_dimensions = collection.dense.dimensions;
    if embeddings.dimensions != expected_dimensions {
        return Err(VectorPointBatchBuildError::DimensionMismatch {
            chunk_id: None,
            expected: expected_dimensions,
            actual: embeddings.dimensions,
        });
    }

    validate_embedding_provenance(document, &embeddings)?;
    let chunks = chunks_by_id(document)?;
    let batch_id = embeddings.batch_id;
    let job_id = embeddings.job_id;
    let provider_id = embeddings.provider_id;
    let model = embeddings.model;
    let mut vectors = vectors_by_chunk_id(embeddings.vectors, &chunks, expected_dimensions)?;
    let mut points = Vec::with_capacity(document.chunks.len());
    for chunk in &document.chunks {
        let vector = vectors.remove(&chunk.chunk_id).ok_or_else(|| {
            VectorPointBatchBuildError::MissingEmbeddingChunk {
                chunk_id: chunk.chunk_id.clone(),
            }
        })?;
        let point_id = stable_point_id(
            &collection.collection,
            &collection.dense.name,
            &document.document_id,
            &chunk.chunk_id,
            &document.generation,
        );
        let payload = build_payload(
            collection,
            document,
            chunk,
            &point_id,
            &batch_id,
            &job_id,
            &provider_id,
            &model,
            context,
        )?;
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

    Ok((points, 0))
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
            actual: embeddings.batch_id,
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

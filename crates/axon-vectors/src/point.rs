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

    /// Like [`build`](Self::build), but also returns the count of chunks
    /// skipped because their payload tripped the secret-redaction
    /// `ForbiddenValue` check (see `point.rs`'s `Payload`-skip branch).
    ///
    /// Callers that surface per-source statistics should use this variant so
    /// redaction-skipped chunks are observable as a count (and, where the
    /// caller has a `SourceWarning` channel, as a warning) rather than only
    /// as a `tracing::warn!` line. The skip count is a publish-stage concern
    /// (it reduces the number of vector points actually upserted) and is
    /// distinct from the preparation-stage `chunks_prepared` count — see
    /// `docs/pipeline-unification/runtime/observability-contract.md`'s
    /// `axon_chunks_prepared_total` vs `axon_vector_points_written_total`.
    pub fn build_with_skipped_count(
        self,
    ) -> Result<(VectorPointBatch, u64), VectorPointBatchBuildError> {
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
        let batch_id = self.embeddings.batch_id;
        let job_id = self.embeddings.job_id;
        let provider_id = self.embeddings.provider_id.clone();
        let model = self.embeddings.model.clone();
        let mut vectors =
            vectors_by_chunk_id(self.embeddings.vectors, &chunks, expected_dimensions)?;
        let mut points = Vec::with_capacity(self.document.chunks.len());
        let mut skipped_redaction = 0u64;

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
                // error is a real defect and still propagates. Count the skip so
                // callers can surface it as a stat/warning instead of only a
                // `tracing::warn!` line that no programmatic consumer sees.
                Err(VectorPointBatchBuildError::Payload {
                    chunk_id,
                    source: crate::payload::VectorPayloadValidationError::ForbiddenValue { .. },
                }) => {
                    skipped_redaction += 1;
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

        Ok((
            VectorPointBatch {
                batch_id,
                collection: self.collection.collection,
                points,
                model,
                dimensions: expected_dimensions,
                sparse_vectors: None,
                payload_indexes: self.collection.payload_indexes,
            },
            skipped_redaction,
        ))
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

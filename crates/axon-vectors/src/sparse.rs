use std::collections::{BTreeMap, BTreeSet};

use axon_api::source::*;

pub(crate) fn validate_sparse_vector(
    point_chunk_id: &ChunkId,
    sparse: &SparseVector,
    stage: axon_error::ErrorStage,
) -> Result<(), ApiError> {
    if sparse.chunk_id != *point_chunk_id {
        return Err(ApiError::new(
            "vector.invalid_sparse_vector",
            stage,
            format!(
                "sparse vector chunk_id {} does not match point chunk_id {}",
                sparse.chunk_id.0, point_chunk_id.0
            ),
        ));
    }
    if sparse.indices.len() != sparse.values.len() {
        return Err(ApiError::new(
            "vector.invalid_sparse_vector",
            stage,
            format!(
                "sparse vector {} has mismatched index/value lengths",
                sparse.chunk_id.0
            ),
        ));
    }
    let mut seen = BTreeSet::new();
    for (index, value) in sparse.indices.iter().zip(sparse.values.iter()) {
        if !seen.insert(*index) {
            return Err(ApiError::new(
                "vector.invalid_sparse_vector",
                stage,
                format!(
                    "sparse vector {} has duplicate index {}",
                    sparse.chunk_id.0, index
                ),
            ));
        }
        if !value.is_finite() {
            return Err(ApiError::new(
                "vector.invalid_sparse_vector",
                stage,
                format!("sparse vector {} has non-finite value", sparse.chunk_id.0),
            ));
        }
    }
    Ok(())
}

pub(crate) fn batch_sparse_vectors_by_chunk(
    batch: &VectorPointBatch,
    stage: axon_error::ErrorStage,
) -> Result<BTreeMap<String, SparseVector>, ApiError> {
    let point_chunks = batch
        .points
        .iter()
        .map(|point| point.chunk_id.0.clone())
        .collect::<BTreeSet<_>>();
    let mut sparse_by_chunk = BTreeMap::new();
    for sparse in batch.sparse_vectors.iter().flatten() {
        if !point_chunks.contains(&sparse.chunk_id.0) {
            return Err(ApiError::new(
                "vector.invalid_sparse_vector",
                stage,
                format!(
                    "batch sparse vector {} has no matching point",
                    sparse.chunk_id.0
                ),
            ));
        }
        if sparse_by_chunk
            .insert(sparse.chunk_id.0.clone(), sparse.clone())
            .is_some()
        {
            return Err(ApiError::new(
                "vector.invalid_sparse_vector",
                stage,
                format!("batch sparse vector {} is duplicated", sparse.chunk_id.0),
            ));
        }
    }
    Ok(sparse_by_chunk)
}

//! Shared write validation for vector store implementations.

use std::collections::{BTreeMap, BTreeSet};

use axon_api::source::*;

use crate::payload::VectorPayload;
use crate::sparse::{batch_sparse_vectors_by_chunk, validate_sparse_vector};
use crate::store::Result;

pub(crate) fn validate_upsert_batch(
    spec: &CollectionSpec,
    batch: &VectorPointBatch,
    stage: axon_error::ErrorStage,
) -> Result<BTreeMap<String, SparseVector>> {
    if batch.dimensions != spec.dense.dimensions {
        return Err(ApiError::new(
            "vector.dimension_mismatch",
            stage,
            format!(
                "batch dimensions {} do not match collection dimensions {}",
                batch.dimensions, spec.dense.dimensions
            ),
        ));
    }
    if batch.collection != spec.collection {
        return Err(ApiError::new(
            "vector.collection_mismatch",
            stage,
            format!(
                "batch collection {} does not match collection spec {}",
                batch.collection, spec.collection
            ),
        ));
    }
    let has_sparse = batch.sparse_vectors.is_some()
        || batch
            .points
            .iter()
            .any(|point| point.sparse_vector.is_some());
    if has_sparse && spec.sparse.is_none() {
        return Err(ApiError::new(
            "vector.sparse_not_configured",
            stage,
            format!(
                "collection {} does not declare a sparse vector namespace",
                batch.collection
            ),
        ));
    }
    let batch_sparse = batch_sparse_vectors_by_chunk(batch, stage)?;
    let mut point_ids = BTreeSet::new();
    let mut chunk_ids = BTreeSet::new();
    let mut batch_job_id: Option<String> = None;
    for point in &batch.points {
        if !point_ids.insert(point.point_id.clone()) {
            return Err(ApiError::new(
                "vector.duplicate_point_id",
                stage,
                format!("batch contains duplicate point id {}", point.point_id.0),
            ));
        }
        if !chunk_ids.insert(point.chunk_id.clone()) {
            return Err(ApiError::new(
                "vector.duplicate_chunk_id",
                stage,
                format!("batch contains duplicate chunk id {}", point.chunk_id.0),
            ));
        }
        let sparse_vector = point
            .sparse_vector
            .as_ref()
            .or_else(|| batch_sparse.get(&point.chunk_id.0));
        if let Some(sparse) = sparse_vector {
            validate_sparse_vector(&point.chunk_id, sparse, stage)?;
        }
        if point.vector.len() as u32 != spec.dense.dimensions {
            return Err(ApiError::new(
                "vector.dimension_mismatch",
                stage,
                format!(
                    "point {} dimensions {} do not match collection dimensions {}",
                    point.point_id.0,
                    point.vector.len(),
                    spec.dense.dimensions
                ),
            ));
        }
        if point.vector.iter().any(|value| !value.is_finite()) {
            return Err(ApiError::new(
                "vector.invalid_dense_vector",
                stage,
                format!(
                    "point {} dense vector contains non-finite values",
                    point.point_id.0
                ),
            ));
        }
        validate_payload_lineage(batch, point, &mut batch_job_id, stage)?;
        VectorPayload::try_from_metadata(point.payload.clone())
            .map_err(|err| ApiError::new("vector.invalid_payload", stage, err.to_string()))?;
    }
    Ok(batch_sparse)
}

fn validate_payload_lineage(
    batch: &VectorPointBatch,
    point: &VectorPoint,
    batch_job_id: &mut Option<String>,
    stage: axon_error::ErrorStage,
) -> Result<()> {
    require_payload_string(point, "collection", stage)
        .and_then(|value| require_equal(point, "collection", value, &batch.collection, stage))?;
    require_payload_string(point, "embedding_batch_id", stage).and_then(|value| {
        require_equal(
            point,
            "embedding_batch_id",
            value,
            &batch.batch_id.0.to_string(),
            stage,
        )
    })?;
    require_payload_string(point, "embedding_model", stage)
        .and_then(|value| require_equal(point, "embedding_model", value, &batch.model, stage))?;
    require_payload_u64(point, "embedding_dimensions", stage).and_then(|value| {
        if value == u64::from(batch.dimensions) {
            Ok(())
        } else {
            Err(lineage_mismatch(
                point,
                "embedding_dimensions",
                value.to_string(),
                batch.dimensions.to_string(),
                stage,
            ))
        }
    })?;
    let job_id = require_payload_string(point, "job_id", stage)?.to_string();
    if let Some(expected) = batch_job_id.as_ref() {
        require_equal(point, "job_id", &job_id, expected, stage)?;
    } else {
        *batch_job_id = Some(job_id);
    }
    Ok(())
}

fn require_payload_string<'a>(
    point: &'a VectorPoint,
    field: &str,
    stage: axon_error::ErrorStage,
) -> Result<&'a str> {
    point
        .payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ApiError::new(
                "vector.payload_lineage_mismatch",
                stage,
                format!(
                    "point {} is missing string payload field {field}",
                    point.point_id.0
                ),
            )
        })
}

fn require_payload_u64(
    point: &VectorPoint,
    field: &str,
    stage: axon_error::ErrorStage,
) -> Result<u64> {
    point
        .payload
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            ApiError::new(
                "vector.payload_lineage_mismatch",
                stage,
                format!(
                    "point {} is missing numeric payload field {field}",
                    point.point_id.0
                ),
            )
        })
}

fn require_equal(
    point: &VectorPoint,
    field: &str,
    actual: &str,
    expected: &str,
    stage: axon_error::ErrorStage,
) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(lineage_mismatch(
            point,
            field,
            actual.to_string(),
            expected.to_string(),
            stage,
        ))
    }
}

fn lineage_mismatch(
    point: &VectorPoint,
    field: &str,
    actual: String,
    expected: String,
    stage: axon_error::ErrorStage,
) -> ApiError {
    ApiError::new(
        "vector.payload_lineage_mismatch",
        stage,
        format!(
            "point {} payload field {field} has {actual}, expected {expected}",
            point.point_id.0
        ),
    )
}

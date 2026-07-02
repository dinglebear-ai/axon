//! REST JSON request bodies sent over reqwest.
//!
//! Split out of `convert.rs` to stay under the 500-line monolith cap; these are
//! the bodies actually posted to Qdrant (the sibling `qdrant_client`-typed
//! builders remain the contract-tested shape validators).

use axon_api::source::*;
use serde_json::json;

use super::QdrantCollectionSettings;
use crate::filter::validate_search_filters;
use crate::filter::{PATH_PREFIX, SEARCH_GENERATION_FIELD};
use crate::store::Result;
use crate::validation::validate_upsert_batch;

fn schema_kind(schema: &PayloadFieldSchema) -> &'static str {
    match schema {
        PayloadFieldSchema::Keyword => "keyword",
        PayloadFieldSchema::Integer => "integer",
        PayloadFieldSchema::Float => "float",
        PayloadFieldSchema::Boolean => "bool",
        PayloadFieldSchema::Datetime => "datetime",
        PayloadFieldSchema::Text => "text",
    }
}

fn distance_kind(distance: VectorDistance) -> &'static str {
    match distance {
        VectorDistance::Cosine => "Cosine",
        VectorDistance::Dot => "Dot",
        VectorDistance::Euclid => "Euclid",
        VectorDistance::Manhattan => "Manhattan",
    }
}

fn sparse_modifier_kind(modifier: SparseVectorModifier) -> &'static str {
    match modifier {
        SparseVectorModifier::None => "none",
        SparseVectorModifier::Idf => "idf",
    }
}

/// REST body for `PUT /collections/{name}` — named dense + optional sparse.
pub fn collection_create_json(spec: &CollectionSpec) -> serde_json::Value {
    let settings = QdrantCollectionSettings::default();
    let mut vectors = serde_json::Map::new();
    vectors.insert(
        spec.dense.name.clone(),
        json!({
            "size": spec.dense.dimensions,
            "distance": distance_kind(spec.dense.distance),
            "on_disk": settings.dense_on_disk,
        }),
    );
    let mut body = json!({
        "vectors": vectors,
        "hnsw_config": {
            "m": settings.hnsw_m,
            "ef_construct": settings.hnsw_ef_construct,
            "on_disk": settings.hnsw_on_disk,
        },
        "quantization_config": {
            "scalar": {
                "type": "int8",
                "quantile": settings.quantization_quantile,
                "always_ram": settings.quantization_always_ram,
            }
        },
        "optimizers_config": { "indexing_threshold": settings.indexing_threshold },
    });
    if let Some(sparse) = &spec.sparse {
        body["sparse_vectors"] = json!({
            sparse.name.clone(): { "modifier": sparse_modifier_kind(sparse.modifier) }
        });
    }
    body
}

/// REST body for `PUT /collections/{name}/index` — one per payload index.
pub fn payload_index_json(index: &PayloadIndexSpec) -> serde_json::Value {
    json!({
        "field_name": index.field_name,
        "field_schema": schema_kind(&index.field_schema),
    })
}

/// REST body for `PUT /collections/{name}/points` — named dense + sparse arms.
pub fn upsert_points_json(
    spec: &CollectionSpec,
    batch: &VectorPointBatch,
) -> Result<serde_json::Value> {
    let batch_sparse = validate_upsert_batch(spec, batch, axon_error::ErrorStage::Upserting)?;
    let points = batch
        .points
        .iter()
        .map(|point| {
            let mut vectors = serde_json::Map::new();
            vectors.insert(spec.dense.name.clone(), json!(point.vector));
            let sparse = point
                .sparse_vector
                .as_ref()
                .or_else(|| batch_sparse.get(&point.chunk_id.0));
            if let (Some(sparse_cfg), Some(sparse)) = (spec.sparse.as_ref(), sparse) {
                vectors.insert(
                    sparse_cfg.name.clone(),
                    json!({ "indices": sparse.indices, "values": sparse.values }),
                );
            }
            json!({
                "id": point.point_id.0,
                "vector": vectors,
                "payload": serde_json::Value::Object(point.payload.0.clone().into_iter().collect()),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "points": points }))
}

/// Search-request filter as REST JSON (generation-fenced on
/// `committed_generation`). Returns `None` when there are no conditions.
pub fn search_filter_json(request: &VectorSearchRequest) -> Result<Option<serde_json::Value>> {
    validate_search_filters(request)?;
    let mut must = Vec::new();
    for (field, value) in request.filters.iter() {
        if field == PATH_PREFIX {
            return Err(ApiError::new(
                "vector.qdrant.path_prefix_unsupported",
                axon_error::ErrorStage::Retrieving,
                "target Qdrant path-prefix filters require live prefix-query wiring",
            ));
        }
        must.push(condition_json(field, value));
    }
    if let Some(generation) = &request.generation {
        must.push(match_json(
            SEARCH_GENERATION_FIELD,
            &serde_json::Value::from(generation.0.clone()),
        ));
    }
    Ok((!must.is_empty()).then(|| json!({ "must": must })))
}

/// Equality filter over a single payload field (used by delete/commit paths).
pub fn eq_filter_json(field: &str, value: &str) -> serde_json::Value {
    json!({ "must": [match_json(field, &serde_json::Value::from(value))] })
}

/// Two-field equality filter (used by generation-scoped commit/delete).
pub fn eq2_filter_json(
    field_a: &str,
    value_a: &str,
    field_b: &str,
    value_b: &str,
) -> serde_json::Value {
    json!({
        "must": [
            match_json(field_a, &serde_json::Value::from(value_a)),
            match_json(field_b, &serde_json::Value::from(value_b)),
        ]
    })
}

/// Prefix or exact filter over the canonical-uri family fields.
pub fn canonical_uri_filter_json(canonical_uri: &str, prefix: bool) -> serde_json::Value {
    let matcher = if prefix {
        json!({ "text": canonical_uri })
    } else {
        json!({ "value": canonical_uri })
    };
    // A bare `should` array already means "match at least one" (Qdrant defaults
    // min_should to 1). Do NOT add a sibling `"min_should": {"min_count": 1}` —
    // Qdrant's MinShould requires the matched conditions nested *inside*
    // min_should, so a sibling form is rejected with HTTP 400.
    json!({
        "should": [
            { "key": "url", "match": matcher.clone() },
            { "key": "source_item_key", "match": matcher.clone() },
            { "key": "chunk_locator.canonical_uri", "match": matcher },
        ],
    })
}

fn condition_json(field: &str, value: &serde_json::Value) -> serde_json::Value {
    let Some(values) = value.as_array() else {
        return match_json(field, value);
    };
    if values.is_empty() {
        return match_json("__axon_match_none", &serde_json::Value::from("__never__"));
    }
    if values.len() == 1 {
        return match_json(field, &values[0]);
    }
    // Bare `should` = OR (min_should defaults to 1). A sibling min_should object
    // is both redundant and malformed for Qdrant's REST filter API.
    json!({
        "should": values.iter().map(|value| match_json(field, value)).collect::<Vec<_>>(),
    })
}

fn match_json(field: &str, value: &serde_json::Value) -> serde_json::Value {
    let matcher = if let Some(int) = value.as_i64() {
        json!({ "value": int })
    } else if let Some(boolean) = value.as_bool() {
        json!({ "value": boolean })
    } else {
        let keyword = match value {
            serde_json::Value::String(value) => value.clone(),
            other => other.to_string(),
        };
        json!({ "value": keyword })
    };
    json!({ "key": field, "match": matcher })
}

#[cfg(test)]
#[path = "rest_tests.rs"]
mod rest_tests;

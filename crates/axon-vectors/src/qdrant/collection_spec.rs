//! Qdrant collection-schema response parsing.

use axon_api::source::*;

use crate::store::Result;

/// Interpret a Qdrant collection GET body into a [`CollectionSpec`].
///
/// Returns `None` when the body lacks a usable dense-vector config (e.g. an
/// error envelope), so callers treat it as "collection absent".
pub(super) fn detect_collection_spec(
    collection: &str,
    body: &serde_json::Value,
    stage: ErrorStage,
) -> Result<Option<CollectionSpec>> {
    let Some(params) = body.pointer("/result/config/params") else {
        return Ok(None);
    };
    let Some(vectors) = params.get("vectors") else {
        return Ok(None);
    };

    // Named-mode: {"vectors": {"<name>": {"size": N, "distance": "Cosine"}}}
    let (dense_name, dense_cfg) = if vectors.get("size").is_some() {
        ("dense".to_string(), vectors.clone())
    } else {
        let Some(object) = vectors.as_object() else {
            return Ok(None);
        };
        let Some((name, cfg)) = object.iter().next() else {
            return Ok(None);
        };
        (name.clone(), cfg.clone())
    };
    let Some(dimensions) = dense_cfg.get("size").and_then(|value| value.as_u64()) else {
        return Ok(None);
    };
    let dimensions = u32::try_from(dimensions)
        .map_err(|_| schema_verification_error(stage, collection, "dense vector dimensions"))?;
    let distance = dense_cfg
        .get("distance")
        .and_then(|value| value.as_str())
        .and_then(parse_distance)
        .ok_or_else(|| schema_verification_error(stage, collection, "dense vector distance"))?;

    let sparse = match params.get("sparse_vectors") {
        None => None,
        Some(value) => parse_sparse_vector_config(value, stage, collection)?,
    };
    let payload_indexes = parse_payload_indexes(body, stage, collection)?;

    Ok(Some(CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: dense_name,
            dimensions,
            distance,
        },
        payload_indexes,
        sparse,
        aliases: Vec::new(),
        distance: None,
        metadata: MetadataMap::new(),
    }))
}

fn parse_sparse_vector_config(
    value: &serde_json::Value,
    stage: ErrorStage,
    collection: &str,
) -> Result<Option<SparseVectorConfig>> {
    let map = value.as_object().ok_or_else(|| {
        schema_verification_error(stage, collection, "sparse vector configuration")
    })?;
    let Some((name, config)) = map.iter().next() else {
        return Ok(None);
    };
    let modifier = match config.get("modifier").and_then(|value| value.as_str()) {
        Some("idf") => SparseVectorModifier::Idf,
        Some("none") => SparseVectorModifier::None,
        _ => {
            return Err(schema_verification_error(
                stage,
                collection,
                "sparse vector modifier",
            ));
        }
    };
    Ok(Some(SparseVectorConfig {
        name: name.clone(),
        modifier,
    }))
}

fn parse_payload_indexes(
    body: &serde_json::Value,
    stage: ErrorStage,
    collection: &str,
) -> Result<Vec<PayloadIndexSpec>> {
    let Some(schema) = body.pointer("/result/payload_schema") else {
        return Ok(Vec::new());
    };
    let schema = schema
        .as_object()
        .ok_or_else(|| schema_verification_error(stage, collection, "payload index schema"))?;
    schema
        .iter()
        .map(|(field, config)| {
            let field_schema = config
                .get("data_type")
                .and_then(|value| value.as_str())
                .and_then(parse_field_schema)
                .ok_or_else(|| {
                    schema_verification_error(
                        stage,
                        collection,
                        &format!("payload index data type for {field}"),
                    )
                })?;
            Ok(PayloadIndexSpec {
                field_name: field.clone(),
                field_schema,
                required_for_filters: true,
            })
        })
        .collect()
}

fn schema_verification_error(stage: ErrorStage, collection: &str, field: &str) -> ApiError {
    ApiError::new(
        "vector.collection_schema_unrecognized",
        stage,
        format!("collection {collection} has an unrecognized or missing {field}"),
    )
}

fn parse_field_schema(data_type: &str) -> Option<PayloadFieldSchema> {
    Some(match data_type {
        "keyword" => PayloadFieldSchema::Keyword,
        "integer" => PayloadFieldSchema::Integer,
        "float" => PayloadFieldSchema::Float,
        "bool" => PayloadFieldSchema::Boolean,
        "datetime" => PayloadFieldSchema::Datetime,
        "text" => PayloadFieldSchema::Text,
        _ => return None,
    })
}

fn parse_distance(value: &str) -> Option<VectorDistance> {
    match value {
        "Cosine" => Some(VectorDistance::Cosine),
        "Dot" => Some(VectorDistance::Dot),
        "Euclid" => Some(VectorDistance::Euclid),
        "Manhattan" => Some(VectorDistance::Manhattan),
        _ => None,
    }
}

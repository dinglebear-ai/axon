//! Request-shape conversion for Qdrant.
//!
//! Two families live here:
//! - `qdrant_client`-typed builders (`qdrant_collection_request`,
//!   `qdrant_upsert_points`, `qdrant_filter`, `qdrant_payload_index_requests`)
//!   are the contract-tested request-shape validators.
//! - `*_json` builders produce the REST bodies actually sent over reqwest.

use std::collections::HashMap;

use axon_api::source::*;
use qdrant_client::qdrant::{
    CreateCollection, CreateFieldIndexCollection, DenseVector, FieldCondition, FieldType, Filter,
    HnswConfigDiff, Match, NamedVectors, OptimizersConfigDiff, PointStruct, QuantizationConfig,
    QuantizationType, ScalarQuantization, SparseVector as QdrantSparseVector,
    SparseVectorConfig as QdrantSparseVectorConfig, SparseVectorParams, Value,
    Vector as QdrantVector, VectorParams, VectorParamsMap, Vectors, VectorsConfig, condition,
    r#match, quantization_config, vector, vectors, vectors_config,
};

use crate::collection::{normalize_collection_spec, validate_collection_spec};
use crate::filter::{PATH_PREFIX, SEARCH_GENERATION_FIELD, validate_search_filters};
use crate::payload::generation_payload_i64;
use crate::store::Result;
use crate::validation::validate_upsert_batch;

// ---------------------------------------------------------------------------
// qdrant_client-typed request builders (contract-tested shape validators)
// ---------------------------------------------------------------------------

pub fn qdrant_collection_request(spec: &CollectionSpec) -> Result<CreateCollection> {
    let spec = normalize_collection_spec(spec.clone());
    validate_collection_spec(&spec)?;
    let settings = QdrantCollectionSettings::default();
    let mut dense = HashMap::new();
    dense.insert(
        spec.dense.name.clone(),
        VectorParams {
            size: u64::from(spec.dense.dimensions),
            distance: qdrant_distance(spec.dense.distance) as i32,
            hnsw_config: None,
            quantization_config: None,
            on_disk: Some(settings.dense_on_disk),
            datatype: None,
            multivector_config: None,
        },
    );

    Ok(CreateCollection {
        collection_name: spec.collection.clone(),
        vectors_config: Some(VectorsConfig {
            config: Some(vectors_config::Config::ParamsMap(VectorParamsMap {
                map: dense,
            })),
        }),
        sparse_vectors_config: spec.sparse.as_ref().map(|sparse| {
            let mut map = HashMap::new();
            map.insert(
                sparse.name.clone(),
                SparseVectorParams {
                    index: None,
                    modifier: Some(qdrant_sparse_modifier(sparse.modifier) as i32),
                },
            );
            QdrantSparseVectorConfig { map }
        }),
        hnsw_config: Some(HnswConfigDiff {
            m: Some(settings.hnsw_m),
            ef_construct: Some(settings.hnsw_ef_construct),
            full_scan_threshold: None,
            max_indexing_threads: None,
            on_disk: Some(settings.hnsw_on_disk),
            payload_m: None,
            inline_storage: None,
        }),
        wal_config: None,
        optimizers_config: Some(OptimizersConfigDiff {
            deleted_threshold: None,
            vacuum_min_vector_number: None,
            default_segment_number: None,
            max_segment_size: None,
            memmap_threshold: None,
            indexing_threshold: Some(settings.indexing_threshold),
            flush_interval_sec: None,
            deprecated_max_optimization_threads: None,
            max_optimization_threads: None,
            prevent_unoptimized: None,
        }),
        shard_number: None,
        on_disk_payload: None,
        timeout: None,
        replication_factor: None,
        write_consistency_factor: None,
        quantization_config: Some(QuantizationConfig {
            quantization: Some(quantization_config::Quantization::Scalar(
                ScalarQuantization {
                    r#type: QuantizationType::Int8 as i32,
                    quantile: Some(settings.quantization_quantile),
                    always_ram: Some(settings.quantization_always_ram),
                },
            )),
        }),
        sharding_method: None,
        strict_mode_config: None,
        metadata: HashMap::new(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QdrantCollectionSettings {
    pub dense_on_disk: bool,
    pub hnsw_m: u64,
    pub hnsw_ef_construct: u64,
    pub hnsw_on_disk: bool,
    pub indexing_threshold: u64,
    pub quantization_quantile: f32,
    pub quantization_always_ram: bool,
}

impl Default for QdrantCollectionSettings {
    fn default() -> Self {
        Self {
            dense_on_disk: true,
            hnsw_m: 32,
            hnsw_ef_construct: 256,
            hnsw_on_disk: false,
            indexing_threshold: 20_000,
            quantization_quantile: 0.99,
            quantization_always_ram: true,
        }
    }
}

pub fn qdrant_payload_index_requests(spec: &CollectionSpec) -> Vec<CreateFieldIndexCollection> {
    normalize_collection_spec(spec.clone())
        .payload_indexes
        .iter()
        .map(|index| CreateFieldIndexCollection {
            collection_name: spec.collection.clone(),
            wait: Some(true),
            field_name: index.field_name.clone(),
            field_type: Some(qdrant_field_type(index.field_schema.clone()) as i32),
            field_index_params: None,
            ordering: None,
            timeout: None,
        })
        .collect()
}

pub fn qdrant_filter(request: &VectorSearchRequest) -> Result<Option<Filter>> {
    validate_search_filters(request)?;
    let mut conditions = Vec::new();
    for (field, value) in request.filters.iter() {
        if field == PATH_PREFIX {
            conditions.push(path_prefix_condition(value));
            continue;
        }
        conditions.extend(qdrant_field_conditions(field, value));
    }
    if let Some(generation) = &request.generation {
        let value =
            serde_json::Value::from(generation_payload_i64(generation, SEARCH_GENERATION_FIELD)?);
        conditions.push(field_condition(SEARCH_GENERATION_FIELD, &value));
    }
    Ok((!conditions.is_empty()).then_some(Filter {
        should: Vec::new(),
        must: conditions,
        must_not: Vec::new(),
        min_should: None,
    }))
}

pub fn qdrant_upsert_points(
    spec: &CollectionSpec,
    batch: &VectorPointBatch,
) -> Result<Vec<PointStruct>> {
    let batch_sparse = validate_upsert_batch(spec, batch, axon_error::ErrorStage::Upserting)?;
    batch
        .points
        .iter()
        .map(|point| {
            let mut named = HashMap::new();
            named.insert(
                spec.dense.name.clone(),
                qdrant_dense_vector(point.vector.clone()),
            );
            let sparse_vector = point
                .sparse_vector
                .as_ref()
                .or_else(|| batch_sparse.get(&point.chunk_id.0));
            if let (Some(sparse_spec), Some(sparse_vector)) = (spec.sparse.as_ref(), sparse_vector)
            {
                named.insert(
                    sparse_spec.name.clone(),
                    qdrant_sparse_vector(
                        sparse_vector.indices.clone(),
                        sparse_vector.values.clone(),
                    ),
                );
            }
            Ok(PointStruct {
                id: Some(point.point_id.0.as_str().into()),
                payload: point
                    .payload
                    .iter()
                    .map(|(field, value)| (field.clone(), json_to_qdrant_value(value)))
                    .collect(),
                vectors: Some(Vectors {
                    vectors_options: Some(vectors::VectorsOptions::Vectors(NamedVectors {
                        vectors: named,
                    })),
                }),
            })
        })
        .collect()
}

#[allow(deprecated)]
fn qdrant_dense_vector(data: Vec<f32>) -> QdrantVector {
    QdrantVector {
        data: Vec::new(),
        indices: None,
        vectors_count: None,
        vector: Some(vector::Vector::Dense(DenseVector { data })),
    }
}

#[allow(deprecated)]
fn qdrant_sparse_vector(indices: Vec<u32>, values: Vec<f32>) -> QdrantVector {
    QdrantVector {
        data: Vec::new(),
        indices: None,
        vectors_count: None,
        vector: Some(vector::Vector::Sparse(QdrantSparseVector {
            indices,
            values,
        })),
    }
}

pub(crate) fn qdrant_field_conditions(
    field: &str,
    value: &serde_json::Value,
) -> Vec<qdrant_client::qdrant::Condition> {
    let Some(values) = value.as_array() else {
        return vec![field_condition(field, value)];
    };
    if values.is_empty() {
        return vec![field_condition(
            "__axon_match_none",
            &serde_json::json!("__never__"),
        )];
    }
    if values.len() == 1 {
        return vec![field_condition(field, &values[0])];
    }
    vec![qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Filter(Filter {
            should: values
                .iter()
                .map(|value| field_condition(field, value))
                .collect(),
            must: Vec::new(),
            must_not: Vec::new(),
            min_should: None,
        })),
    }]
}

fn field_condition(field: &str, value: &serde_json::Value) -> qdrant_client::qdrant::Condition {
    qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Field(FieldCondition {
            key: field.to_string(),
            r#match: Some(Match {
                match_value: Some(qdrant_match_value(value)),
            }),
            range: None,
            geo_bounding_box: None,
            geo_radius: None,
            values_count: None,
            geo_polygon: None,
            datetime_range: None,
            is_empty: None,
            is_null: None,
        })),
    }
}

fn path_prefix_condition(value: &serde_json::Value) -> qdrant_client::qdrant::Condition {
    qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Filter(Filter {
            should: ["source_item_key", "chunk_locator.path"]
                .into_iter()
                .map(|field| text_field_condition(field, value))
                .collect(),
            must: Vec::new(),
            must_not: Vec::new(),
            min_should: None,
        })),
    }
}

fn text_field_condition(
    field: &str,
    value: &serde_json::Value,
) -> qdrant_client::qdrant::Condition {
    let text = match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    };
    qdrant_client::qdrant::Condition {
        condition_one_of: Some(condition::ConditionOneOf::Field(FieldCondition {
            key: field.to_string(),
            r#match: Some(Match {
                match_value: Some(r#match::MatchValue::Text(text)),
            }),
            range: None,
            geo_bounding_box: None,
            geo_radius: None,
            values_count: None,
            geo_polygon: None,
            datetime_range: None,
            is_empty: None,
            is_null: None,
        })),
    }
}

fn qdrant_match_value(value: &serde_json::Value) -> r#match::MatchValue {
    if let Some(value) = value.as_i64() {
        return r#match::MatchValue::Integer(value);
    }
    if let Some(value) = value.as_bool() {
        return r#match::MatchValue::Boolean(value);
    }
    r#match::MatchValue::Keyword(match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    })
}

fn json_to_qdrant_value(value: &serde_json::Value) -> Value {
    serde_json::from_value(value.clone()).unwrap_or_else(|_| Value {
        kind: Some(qdrant_client::qdrant::value::Kind::StringValue(
            value.to_string(),
        )),
    })
}

fn qdrant_distance(distance: VectorDistance) -> qdrant_client::qdrant::Distance {
    match distance {
        VectorDistance::Cosine => qdrant_client::qdrant::Distance::Cosine,
        VectorDistance::Dot => qdrant_client::qdrant::Distance::Dot,
        VectorDistance::Euclid => qdrant_client::qdrant::Distance::Euclid,
        VectorDistance::Manhattan => qdrant_client::qdrant::Distance::Manhattan,
    }
}

fn qdrant_sparse_modifier(modifier: SparseVectorModifier) -> qdrant_client::qdrant::Modifier {
    match modifier {
        SparseVectorModifier::None => qdrant_client::qdrant::Modifier::None,
        SparseVectorModifier::Idf => qdrant_client::qdrant::Modifier::Idf,
    }
}

fn qdrant_field_type(schema: PayloadFieldSchema) -> FieldType {
    match schema {
        PayloadFieldSchema::Keyword => FieldType::Keyword,
        PayloadFieldSchema::Integer => FieldType::Integer,
        PayloadFieldSchema::Float => FieldType::Float,
        PayloadFieldSchema::Boolean => FieldType::Bool,
        PayloadFieldSchema::Datetime => FieldType::Datetime,
        PayloadFieldSchema::Text => FieldType::Text,
    }
}

mod rest;
pub use rest::{
    canonical_uri_filter_json, collection_create_json, eq_filter_json, eq2_filter_json,
    payload_index_json, search_filter_json, upsert_points_json,
};

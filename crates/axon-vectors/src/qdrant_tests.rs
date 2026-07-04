use axon_api::source::*;
use qdrant_client::qdrant::{
    FieldType, condition, r#match, point_id, quantization_config, vector, vectors, vectors_config,
};
use serde_json::json;

use crate::filter::SEARCH_GENERATION_FIELD;
use crate::point::VectorPointBatchBuilder;
use crate::qdrant::{
    QdrantVectorStore, qdrant_collection_request, qdrant_filter, qdrant_payload_index_requests,
    qdrant_upsert_points,
};
use crate::store::VectorStore;
use crate::testing::{
    test_collection_spec, test_collection_spec_hybrid, test_embedding_result_for,
    test_prepared_document, test_vector_build_context,
};

#[test]
fn collection_spec_converts_to_named_dense_and_optional_sparse_config() {
    let mut spec = test_collection_spec(3);
    spec.dense.name = "dense_docs".to_string();
    spec.sparse = Some(SparseVectorConfig {
        name: "bm42".to_string(),
        modifier: SparseVectorModifier::Idf,
    });

    let request = qdrant_collection_request(&spec).unwrap();

    assert_eq!(request.collection_name, "axon-test");
    let vectors = request.vectors_config.unwrap();
    let vectors_config::Config::ParamsMap(map) = vectors.config.unwrap() else {
        panic!("expected named dense vector params");
    };
    assert_eq!(map.map["dense_docs"].size, 3);
    assert_eq!(
        map.map["dense_docs"].distance,
        qdrant_client::qdrant::Distance::Cosine as i32
    );
    assert_eq!(map.map["dense_docs"].on_disk, Some(true));
    let hnsw = request.hnsw_config.unwrap();
    assert_eq!(hnsw.m, Some(32));
    assert_eq!(hnsw.ef_construct, Some(256));
    assert_eq!(hnsw.on_disk, Some(false));
    let quantization = request.quantization_config.unwrap().quantization.unwrap();
    let quantization_config::Quantization::Scalar(scalar) = quantization else {
        panic!("expected scalar quantization");
    };
    assert_eq!(
        scalar.r#type,
        qdrant_client::qdrant::QuantizationType::Int8 as i32
    );
    assert_eq!(scalar.quantile, Some(0.99));
    assert_eq!(scalar.always_ram, Some(true));

    let sparse = request.sparse_vectors_config.unwrap();
    assert_eq!(
        sparse.map["bm42"].modifier,
        Some(qdrant_client::qdrant::Modifier::Idf as i32)
    );
}

#[test]
fn collection_spec_rejects_zero_dimensions_before_qdrant_conversion() {
    let mut spec = test_collection_spec(3);
    spec.dense.dimensions = 0;

    let err = qdrant_collection_request(&spec).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.collection_drift");
}

#[test]
fn payload_index_specs_convert_to_qdrant_index_requests() {
    let spec = test_collection_spec(3);
    let indexes = qdrant_payload_index_requests(&spec);

    assert!(indexes.iter().any(|index| {
        index.collection_name == "axon-test"
            && index.field_name == "source_generation"
            && index.field_type == Some(FieldType::Keyword as i32)
            && index.wait == Some(true)
    }));
    assert!(indexes.iter().any(|index| {
        index.field_name == "source_id" && index.field_type == Some(FieldType::Keyword as i32)
    }));
}

#[test]
fn source_generation_and_document_filters_convert_to_qdrant_filters() {
    let mut request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [
                ("source_id".to_string(), json!("src-web")),
                ("document_id".to_string(), json!("doc-web")),
            ]
            .into_iter()
            .collect(),
        ),
        hybrid: None,
        generation: Some(SourceGenerationId::new("7")),
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = qdrant_filter(&request).unwrap().unwrap();
    assert_eq!(filter.must.len(), 3);
    let keys = filter
        .must
        .iter()
        .map(|condition| {
            let condition::ConditionOneOf::Field(field) =
                condition.condition_one_of.as_ref().unwrap()
            else {
                panic!("expected field condition");
            };
            field.key.as_str()
        })
        .collect::<Vec<_>>();
    assert!(keys.contains(&"source_id"));
    assert!(keys.contains(&SEARCH_GENERATION_FIELD));
    assert!(!keys.contains(&"source_generation"));
    assert!(keys.contains(&"document_id"));
    let generation_condition = filter
        .must
        .iter()
        .find_map(|condition| {
            let condition::ConditionOneOf::Field(field) = condition.condition_one_of.as_ref()?
            else {
                return None;
            };
            (field.key == SEARCH_GENERATION_FIELD).then_some(field)
        })
        .expect("search generation condition");
    assert!(matches!(
        generation_condition
            .r#match
            .as_ref()
            .and_then(|value| value.match_value.as_ref()),
        Some(r#match::MatchValue::Keyword(value)) if value == "7"
    ));

    request.filters.clear();
    request.generation = None;
    assert!(qdrant_filter(&request).unwrap().is_none());
}

#[test]
fn qdrant_filter_rejects_path_prefix_until_live_prefix_wiring_exists() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("path_prefix".to_string(), json!("src"))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let err = qdrant_filter(&request).unwrap_err();

    assert_eq!(
        err.code.to_string(),
        "vector.qdrant.path_prefix_unsupported"
    );
}

#[test]
fn opaque_generation_filter_is_converted_as_keyword() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: None,
        generation: Some(SourceGenerationId::new("gen-7")),
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = qdrant_filter(&request).unwrap().unwrap();
    let condition::ConditionOneOf::Field(field) = filter.must[0].condition_one_of.as_ref().unwrap()
    else {
        panic!("expected search generation field condition");
    };
    assert_eq!(field.key, SEARCH_GENERATION_FIELD);

    assert!(matches!(
        field
            .r#match
            .as_ref()
            .and_then(|value| value.match_value.as_ref()),
        Some(r#match::MatchValue::Keyword(value)) if value == "gen-7"
    ));
}

#[test]
fn unsupported_filter_value_shapes_are_rejected_before_qdrant_conversion() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("source_id".to_string(), json!({"eq": "src-web"}))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let err = qdrant_filter(&request).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.invalid_filter_value");
}

#[test]
fn float_filter_values_are_rejected_before_qdrant_conversion() {
    for value in [json!(0.42), json!([1, 2.5])] {
        let request = VectorSearchRequest {
            collection: "axon-test".to_string(),
            query: "docs".to_string(),
            limit: 10,
            dense_vector: None,
            sparse_vector: None,
            filters: MetadataMap(
                [("graph_confidence".to_string(), value)]
                    .into_iter()
                    .collect(),
            ),
            hybrid: None,
            generation: None,
            graph_refs: Vec::new(),
            metadata: MetadataMap::new(),
        };

        let err = qdrant_filter(&request).unwrap_err();

        assert_eq!(err.code.to_string(), "vector.invalid_filter_value");
    }
}

#[test]
fn array_filter_values_convert_to_qdrant_should_groups() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("vector_namespace".to_string(), json!(["docs", "guides"]))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = qdrant_filter(&request).unwrap().unwrap();
    assert_eq!(filter.must.len(), 1);
    let condition::ConditionOneOf::Filter(namespace_filter) =
        filter.must[0].condition_one_of.as_ref().unwrap()
    else {
        panic!("expected nested OR filter");
    };
    let keys = namespace_filter
        .should
        .iter()
        .map(|condition| {
            let condition::ConditionOneOf::Field(field) =
                condition.condition_one_of.as_ref().unwrap()
            else {
                panic!("expected field condition");
            };
            (
                field.key.as_str(),
                field
                    .r#match
                    .as_ref()
                    .and_then(|value| value.match_value.as_ref()),
            )
        })
        .collect::<Vec<_>>();
    assert!(keys.iter().any(|(key, value)| {
        *key == "vector_namespace"
            && matches!(value, Some(r#match::MatchValue::Keyword(value)) if value == "docs")
    }));
    assert!(keys.iter().any(|(key, value)| {
        *key == "vector_namespace"
            && matches!(value, Some(r#match::MatchValue::Keyword(value)) if value == "guides")
    }));
}

#[test]
fn empty_array_filter_values_convert_to_match_none_qdrant_filter() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("vector_namespace".to_string(), json!([]))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = qdrant_filter(&request).unwrap().unwrap();
    assert_eq!(filter.must.len(), 1);
    let condition::ConditionOneOf::Field(field) = filter.must[0].condition_one_of.as_ref().unwrap()
    else {
        panic!("expected match-none field condition");
    };
    assert_eq!(field.key, "__axon_match_none");
    assert!(matches!(
        field
            .r#match
            .as_ref()
            .and_then(|value| value.match_value.as_ref()),
        Some(r#match::MatchValue::Keyword(value)) if value == "__never__"
    ));
}

#[test]
fn vector_point_batch_converts_to_qdrant_points_without_dropping_payload_fields() {
    let mut spec = test_collection_spec_hybrid(3);
    spec.dense.name = "dense_docs".to_string();
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();

    let points = qdrant_upsert_points(&spec, &batch).unwrap();

    assert_eq!(points.len(), 2);
    let first = &points[0];
    let point_id::PointIdOptions::Uuid(uuid) = first
        .id
        .as_ref()
        .unwrap()
        .point_id_options
        .as_ref()
        .unwrap()
    else {
        panic!("expected UUID point id");
    };
    assert_eq!(uuid, &batch.points[0].point_id.0);
    let vectors::VectorsOptions::Vectors(named) = first
        .vectors
        .as_ref()
        .unwrap()
        .vectors_options
        .as_ref()
        .unwrap()
    else {
        panic!("expected named vectors");
    };
    assert!(!named.vectors.contains_key("dense"));
    let vector::Vector::Dense(dense) = named.vectors["dense_docs"].vector.as_ref().unwrap() else {
        panic!("expected dense vector");
    };
    assert_eq!(dense.data, batch.points[0].vector);
    assert!(first.payload.contains_key("source_id"));
    for key in batch.points[0].payload.keys() {
        assert!(first.payload.contains_key(key), "{key} was dropped");
    }
}

#[test]
fn vector_point_batch_converts_sparse_vectors_to_qdrant_named_sparse_arm() {
    let mut spec = test_collection_spec(3);
    spec.dense.name = "dense_docs".to_string();
    spec.sparse = Some(SparseVectorConfig {
        name: "bm42".to_string(),
        modifier: SparseVectorModifier::Idf,
    });
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.points[0].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1, 3, 8],
        values: vec![0.2, 0.4, 0.9],
    });

    let points = qdrant_upsert_points(&spec, &batch).unwrap();
    let vectors::VectorsOptions::Vectors(named) = points[0]
        .vectors
        .as_ref()
        .unwrap()
        .vectors_options
        .as_ref()
        .unwrap()
    else {
        panic!("expected named vectors");
    };

    assert!(named.vectors.contains_key("dense_docs"));
    let vector::Vector::Sparse(sparse) = named.vectors["bm42"].vector.as_ref().unwrap() else {
        panic!("expected sparse vector");
    };
    assert_eq!(sparse.indices, vec![1, 3, 8]);
    assert_eq!(sparse.values, vec![0.2, 0.4, 0.9]);
}

#[test]
fn sparse_vectors_for_dense_only_collections_are_rejected_before_qdrant_conversion() {
    let mut spec = test_collection_spec(3);
    spec.dense.name = "dense_docs".to_string();
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.points[0].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1],
        values: vec![0.2],
    });

    let err = qdrant_upsert_points(&spec, &batch).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.sparse_not_configured");
}

#[test]
fn vector_point_batch_merges_batch_level_sparse_vectors_by_chunk_id() {
    let mut spec = test_collection_spec_hybrid(3);
    spec.dense.name = "dense_docs".to_string();
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    for point in &mut batch.points {
        point.sparse_vector = None;
    }
    batch.sparse_vectors = Some(vec![SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![2],
        values: vec![0.7],
    }]);

    let points = qdrant_upsert_points(&spec, &batch).unwrap();
    let vectors::VectorsOptions::Vectors(named) = points[0]
        .vectors
        .as_ref()
        .unwrap()
        .vectors_options
        .as_ref()
        .unwrap()
    else {
        panic!("expected named vectors");
    };

    let vector::Vector::Sparse(sparse) = named.vectors["bm42"].vector.as_ref().unwrap() else {
        panic!("expected sparse vector");
    };
    assert_eq!(sparse.indices, vec![2]);
    assert_eq!(sparse.values, vec![0.7]);
}

#[test]
fn malformed_sparse_vectors_are_rejected_before_qdrant_conversion() {
    let mut spec = test_collection_spec(3);
    spec.sparse = Some(SparseVectorConfig {
        name: "bm42".to_string(),
        modifier: SparseVectorModifier::Idf,
    });
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.points[0].sparse_vector = Some(SparseVector {
        chunk_id: batch.points[0].chunk_id.clone(),
        indices: vec![1, 1],
        values: vec![0.2, 0.3],
    });

    let err = qdrant_upsert_points(&spec, &batch).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.invalid_sparse_vector");
}

#[test]
fn invalid_payloads_are_rejected_before_qdrant_conversion() {
    let spec = test_collection_spec_hybrid(3);
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.points[0].payload.remove("chunk_locator");

    let err = qdrant_upsert_points(&spec, &batch).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.invalid_payload");
}

#[test]
fn duplicate_points_are_rejected_before_qdrant_conversion() {
    let spec = test_collection_spec_hybrid(3);
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let mut batch = VectorPointBatchBuilder::new(
        spec.clone(),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    batch.points[1].point_id = batch.points[0].point_id.clone();

    let err = qdrant_upsert_points(&spec, &batch).unwrap_err();

    assert_eq!(err.code.to_string(), "vector.duplicate_point_id");
}

// Unreachable endpoint: connection is refused quickly, so live calls fail with
// a redaction-safe transport error rather than blocking on the request timeout.
// `hunter2` and the raw URL must never appear in any surfaced error detail.
const UNREACHABLE_QDRANT_URL: &str = "http://token:secret@127.0.0.1:1/path?api_key=hunter2";

fn assert_redacted(err: &ApiError) {
    assert_eq!(err.provider_id.as_deref(), Some("target-qdrant"));
    assert_eq!(
        err.details.get("endpoint").map(String::as_str),
        Some("configured")
    );
    assert!(
        !err.message.contains("hunter2") && !err.message.contains("secret"),
        "message leaked credentials: {}",
        err.message
    );
    assert!(
        !err.message.contains(UNREACHABLE_QDRANT_URL),
        "message leaked raw url: {}",
        err.message
    );
    for value in err.details.values() {
        assert!(!value.contains("hunter2"), "detail leaked api key: {value}");
        assert!(
            !value.contains(UNREACHABLE_QDRANT_URL),
            "detail leaked raw url: {value}"
        );
    }
}

#[tokio::test]
async fn qdrant_vector_store_live_calls_surface_redaction_safe_transport_errors() {
    let store = QdrantVectorStore::new(UNREACHABLE_QDRANT_URL, "target-qdrant");

    let err = store
        .ensure_collection(test_collection_spec(3))
        .await
        .unwrap_err();
    assert_redacted(&err);

    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 3);
    let batch = VectorPointBatchBuilder::new(
        test_collection_spec(3),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    let delete = VectorDeleteSelector::Chunks {
        collection: "axon-test".to_string(),
        chunk_ids: vec![ChunkId::new("chunk-web-1")],
    };
    let search = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: Some(vec![1.0, 0.0, 0.0]),
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: Some(false),
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };
    for err in [
        store.upsert(batch).await.unwrap_err(),
        store
            .mark_generation_committed(
                "axon-test".to_string(),
                SourceId::new("src-web"),
                SourceGenerationId::new("7"),
            )
            .await
            .unwrap_err(),
        store.delete(delete).await.unwrap_err(),
        store.search(search).await.unwrap_err(),
    ] {
        assert_redacted(&err);
    }
}

#[tokio::test]
async fn qdrant_capabilities_report_generation_publish_and_redact_on_probe_failure() {
    let store = QdrantVectorStore::new(UNREACHABLE_QDRANT_URL, "target-qdrant");
    let capability = store.capabilities().await.unwrap();

    assert_eq!(capability.provider_kind, ProviderKind::Vector);
    assert_eq!(capability.implementation, "qdrant");
    // The instance is unreachable, so the liveness probe downgrades health.
    assert_eq!(capability.health, HealthStatus::Unavailable);
    let last_error = capability.last_error.as_ref().unwrap();
    assert_redacted(last_error);

    let vector_store = capability.vector_store.unwrap();
    assert!(vector_store.dense);
    assert!(vector_store.sparse);
    assert!(vector_store.hybrid);
    // Generation-aware publish is now supported by the live store.
    assert!(vector_store.generation_publish);
}

//! Regression tests for the raw-JSON Qdrant filter builders.
//!
//! These pin the wire shape of OR filters: a bare `should` array (which Qdrant
//! treats as min_should = 1) with NO sibling `min_should` object. A sibling
//! `"min_should": {"min_count": 1}` is malformed for Qdrant's REST filter API
//! (MinShould requires the conditions nested inside it) and is rejected with
//! HTTP 400 at runtime — a defect unit tests over self-constructed JSON missed.

use super::*;

#[test]
fn collection_create_json_serializes_runtime_settings() {
    let spec = CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 3,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: None,
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    };
    let settings = QdrantCollectionSettings {
        dense_on_disk: true,
        hnsw_m: 16,
        hnsw_ef_construct: 100,
        hnsw_on_disk: false,
        indexing_threshold: 9_999_999,
        quantization_enabled: true,
        quantization_quantile: 0.98,
        quantization_always_ram: true,
    };

    let body = collection_create_json_with_settings(&spec, settings);

    assert_eq!(body["hnsw_config"]["m"], 16);
    assert_eq!(body["hnsw_config"]["ef_construct"], 100);
    assert_eq!(body["optimizers_config"]["indexing_threshold"], 9_999_999);
    assert_eq!(body["quantization_config"]["scalar"]["type"], "int8");
    let quantile = body["quantization_config"]["scalar"]["quantile"]
        .as_f64()
        .expect("quantile number");
    assert!((quantile - 0.98).abs() < 1e-6);
    assert_eq!(body["quantization_config"]["scalar"]["always_ram"], true);
}

#[test]
fn upsert_points_body_serializes_dense_sparse_and_payload_without_shape_drift() {
    let spec = CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 2,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    };
    let sparse = SparseVector {
        chunk_id: ChunkId::new("chunk-1"),
        indices: vec![7, 11],
        values: vec![0.5, 1.25],
    };
    let point = VectorPoint {
        point_id: VectorPointId::new("point-1"),
        chunk_id: ChunkId::new("chunk-1"),
        vector: vec![0.1, 0.2],
        sparse_vector: None,
        payload: MetadataMap(
            [("source_id".to_string(), serde_json::json!("src-1"))]
                .into_iter()
                .collect(),
        ),
    };
    let sparse_by_chunk = [("chunk-1", &sparse)].into_iter().collect();

    // Exercise the production wire path. serde_json::to_value widens f32 to
    // f64 in its intermediate Value representation, which creates misleading
    // precision drift even though to_writer emits the compact f32 JSON sent
    // to Qdrant.
    let encoded = serde_json::to_vec(&UpsertPointsBody::new(
        &spec,
        std::slice::from_ref(&point),
        &sparse_by_chunk,
    ))
    .expect("encode borrowing upsert body");
    let value: serde_json::Value =
        serde_json::from_slice(&encoded).expect("parse encoded qdrant upsert body");

    assert_eq!(
        value,
        serde_json::json!({
            "points": [{
                "id": "point-1",
                "vector": {
                    "dense": [0.1, 0.2],
                    "bm42": {"indices": [7, 11], "values": [0.5, 1.25]}
                },
                "payload": {"source_id": "src-1"}
            }]
        })
    );
}

#[test]
fn canonical_uri_filter_has_bare_should_without_min_should() {
    let filter = canonical_uri_filter_json("https://example.com/docs", false);
    let should = filter
        .get("should")
        .and_then(|value| value.as_array())
        .expect("canonical-uri filter must expose a `should` array");
    assert_eq!(
        should.len(),
        4,
        "item/source/source-key/chunk canonical URI arms"
    );
    let keys = should
        .iter()
        .filter_map(|condition| condition["key"].as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"item_canonical_uri"));
    assert!(keys.contains(&"source_canonical_uri"));
    assert!(keys.contains(&"source_item_key"));
    assert!(keys.contains(&"chunk_locator.canonical_uri"));
    assert!(!keys.contains(&"url"));
    assert!(
        filter.get("min_should").is_none(),
        "must NOT emit a sibling min_should object (Qdrant 400s on it): {filter}"
    );
}

#[test]
fn multi_value_condition_is_bare_should_without_min_should() {
    let value = serde_json::json!(["rust", "python", "go"]);
    let condition = condition_json("code_language", &value);
    let should = condition
        .get("should")
        .and_then(|value| value.as_array())
        .expect("multi-value OR condition must expose a `should` array");
    assert_eq!(should.len(), 3);
    assert!(
        condition.get("min_should").is_none(),
        "multi-value OR condition must NOT emit a sibling min_should: {condition}"
    );
}

#[test]
fn single_value_condition_is_a_flat_match() {
    let condition = condition_json("code_language", &serde_json::json!(["rust"]));
    assert!(
        condition.get("should").is_none(),
        "a single-value filter collapses to a flat match, not a should array"
    );
    assert_eq!(
        condition.get("key").and_then(|value| value.as_str()),
        Some("code_language")
    );
}

#[test]
fn search_filter_json_converts_path_prefix_to_source_path_should_filter() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [("path_prefix".to_string(), serde_json::json!("src"))]
                .into_iter()
                .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = search_filter_json(&request)
        .expect("path prefix filter")
        .expect("filter");
    let path_filter = filter["must"][0]["should"]
        .as_array()
        .expect("path should array");

    assert_eq!(path_filter.len(), 2);
    assert_eq!(path_filter[0]["key"], "source_item_key");
    assert_eq!(path_filter[1]["key"], "chunk_locator.path");
    assert_eq!(path_filter[0]["match"]["text"], "src");
}

#[test]
fn datetime_range_filter_uses_qdrant_range_wire_shape() {
    let request = VectorSearchRequest {
        collection: "axon-test".to_string(),
        query: "docs".to_string(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap(
            [(
                "embedded_at".to_string(),
                serde_json::json!({
                    "gte": "2026-07-01T00:00:00Z",
                    "lte": "2026-07-31T23:59:59Z"
                }),
            )]
            .into_iter()
            .collect(),
        ),
        hybrid: None,
        generation: None,
        graph_refs: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let filter = search_filter_json(&request)
        .expect("datetime range filter")
        .expect("filter");
    assert_eq!(filter["must"][0]["key"], "embedded_at");
    assert_eq!(
        filter["must"][0]["range"],
        serde_json::json!({
            "gte": "2026-07-01T00:00:00Z",
            "lte": "2026-07-31T23:59:59Z"
        })
    );
}

use serde_json::json;

use super::*;

#[test]
fn collection_spec_round_trips_with_named_dense_and_sparse_config() {
    let spec = CollectionSpec {
        collection: "axon".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 1024,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![PayloadIndexSpec {
            field_name: "source_id".to_string(),
            field_schema: PayloadFieldSchema::Keyword,
            required_for_filters: true,
        }],
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: vec!["default".to_string()],
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    };

    let value = serde_json::to_value(&spec).expect("serialize collection spec");
    assert_eq!(value["dense"]["name"], "dense");
    assert_eq!(value["dense"]["distance"], "cosine");
    assert_eq!(value["sparse"]["modifier"], "idf");

    assert_eq!(
        serde_json::from_value::<CollectionSpec>(value).expect("deserialize collection spec"),
        spec
    );
}

#[test]
fn vector_delete_selector_round_trips_tagged_variants() {
    let generation = VectorDeleteSelector::Generation {
        collection: "axon".to_string(),
        source_id: SourceId::from("src_local"),
        generation: SourceGenerationId::from("gen_0001"),
    };
    let points = VectorDeleteSelector::Points {
        collection: "axon".to_string(),
        point_ids: vec![VectorPointId::from("point_1")],
    };
    let filter = VectorDeleteSelector::Filter {
        collection: "axon".to_string(),
        filter: json!({"must": [{"key": "source_id", "match": {"value": "src_local"}}]}),
    };

    assert_eq!(
        serde_json::to_value(&generation).unwrap()["kind"],
        "generation"
    );
    assert_eq!(
        serde_json::from_value::<VectorDeleteSelector>(serde_json::to_value(&points).unwrap())
            .unwrap(),
        points
    );
    assert_eq!(
        serde_json::from_value::<VectorDeleteSelector>(serde_json::to_value(&filter).unwrap())
            .unwrap(),
        filter
    );
}

#[test]
fn vector_search_request_and_result_round_trip() {
    let request = VectorSearchRequest {
        collection: "axon".to_string(),
        query: "source DTO vector contract".to_string(),
        limit: 10,
        dense_vector: Some(vec![0.1, 0.2]),
        sparse_vector: Some(SparseVector {
            chunk_id: ChunkId::from("query"),
            indices: vec![3, 8],
            values: vec![0.7, 0.4],
        }),
        filters: MetadataMap::new(),
        hybrid: Some(true),
        generation: Some(SourceGenerationId::from("gen_0002")),
        graph_refs: vec![GraphNodeId::from("node_1")],
        metadata: MetadataMap::new(),
    };
    let result = VectorSearchResult {
        collection: "axon".to_string(),
        results: vec![VectorSearchMatch {
            point_id: VectorPointId::from("point_1"),
            score: 0.91,
            chunk_id: Some(ChunkId::from("chunk_1")),
            document_id: Some(DocumentId::from("doc_1")),
            source_id: Some(SourceId::from("src_local")),
            source_item_key: Some(SourceItemKey::from("README.md")),
            text: Some("source DTO vector contract".to_string()),
            payload: MetadataMap::new(),
        }],
        limit: 10,
        next_cursor: Some("cursor_2".to_string()),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };

    assert_eq!(
        serde_json::from_value::<VectorSearchRequest>(serde_json::to_value(&request).unwrap())
            .unwrap(),
        request
    );
    assert_eq!(
        serde_json::from_value::<VectorSearchResult>(serde_json::to_value(&result).unwrap())
            .unwrap(),
        result
    );
}

#[test]
fn vector_store_delete_result_round_trips() {
    let result = VectorStoreDeleteResult {
        collection: "axon".to_string(),
        points_matched: 12,
        points_deleted: 10,
        dry_run: false,
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };

    assert_eq!(
        serde_json::from_value::<VectorStoreDeleteResult>(serde_json::to_value(&result).unwrap())
            .unwrap(),
        result
    );
}

#[test]
fn vector_operation_dtos_reject_unknown_fields() {
    let collection_err = serde_json::from_value::<CollectionSpec>(json!({
        "collection": "axon",
        "dense": {"name": "dense", "dimensions": 1024, "distance": "cosine"},
        "payload_indexes": [],
        "aliases": [],
        "metadata": {},
        "unexpected": true
    }))
    .expect_err("collection spec must reject unknown fields");
    assert!(
        collection_err.to_string().contains("unknown field"),
        "{collection_err}"
    );

    let selector_err = serde_json::from_value::<VectorDeleteSelector>(json!({
        "kind": "points",
        "collection": "axon",
        "point_ids": ["point_1"],
        "unexpected": true
    }))
    .expect_err("delete selector must reject unknown fields");
    assert!(
        selector_err.to_string().contains("unknown field"),
        "{selector_err}"
    );

    let search_err = serde_json::from_value::<VectorSearchRequest>(json!({
        "collection": "axon",
        "query": "dto",
        "limit": 5,
        "filters": {},
        "graph_refs": [],
        "metadata": {},
        "unexpected": true
    }))
    .expect_err("search request must reject unknown fields");
    assert!(
        search_err.to_string().contains("unknown field"),
        "{search_err}"
    );
}

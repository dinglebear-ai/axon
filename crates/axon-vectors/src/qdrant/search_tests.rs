use super::*;

#[test]
fn ordinary_dense_search_uses_ann() {
    let body = named_dense_body(&[0.1, 0.2], "dense", 5, None);
    assert_eq!(body["params"]["exact"], false);
    assert_eq!(body["params"]["hnsw_ef"], HNSW_EF_SEARCH);
}

#[test]
fn hybrid_dense_prefetch_uses_ann() {
    let sparse = SparseVector {
        chunk_id: ChunkId::new("chunk-1"),
        indices: vec![1],
        values: vec![0.5],
    };
    let body = hybrid_body(&[0.1, 0.2], "dense", &sparse, "bm42", 5, None, None);
    assert_eq!(body["prefetch"][0]["params"]["exact"], false);
    assert_eq!(body["prefetch"][0]["params"]["hnsw_ef"], HNSW_EF_SEARCH);
}

#[test]
fn hybrid_prefetch_honors_explicit_candidate_window() {
    let sparse = SparseVector {
        chunk_id: ChunkId::new("chunk-1"),
        indices: vec![1],
        values: vec![0.5],
    };
    let body = hybrid_body(&[0.1, 0.2], "dense", &sparse, "bm42", 5, Some(37), None);

    assert_eq!(body["prefetch"][0]["limit"], 37);
    assert_eq!(body["prefetch"][1]["limit"], 37);
    assert_eq!(body["limit"], 5);
}

#[test]
fn hybrid_prefetch_cannot_underfill_limit_and_preserves_global_filter() {
    let sparse = SparseVector {
        chunk_id: ChunkId::new("query"),
        indices: vec![1],
        values: vec![0.5],
    };
    let filter =
        serde_json::json!({"must": [{"key": "committed_generation", "match": {"value": 7}}]});
    let body = hybrid_body(
        &[0.1, 0.2],
        "dense",
        &sparse,
        "bm42",
        20,
        Some(3),
        Some(&filter),
    );
    assert_eq!(body["filter"], filter);
    for arm in body["prefetch"].as_array().unwrap() {
        assert_eq!(arm["limit"], 20);
    }
}

#[tokio::test]
async fn empty_sparse_arm_uses_dense_wire_query() {
    let server = httpmock::MockServer::start_async().await;
    let request = VectorSearchRequest {
        collection: "axon-test".into(),
        query: "the and".into(),
        limit: 5,
        dense_vector: Some(vec![0.1, 0.2]),
        sparse_vector: Some(SparseVector {
            chunk_id: ChunkId::new("query"),
            indices: vec![],
            values: vec![],
        }),
        filters: MetadataMap::new(),
        hybrid: Some(true),
        generation: None,
        graph_refs: vec![],
        metadata: MetadataMap::new(),
    };
    let filter = search_filter_json(&request).unwrap();
    let expected = named_dense_body(&[0.1, 0.2], "dense", 5, filter.as_ref());
    let mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/collections/axon-test/points/query")
                .json_body(expected);
            then.status(200)
                .json_body(serde_json::json!({"result":{"points":[]}}));
        })
        .await;
    let http = QdrantHttp::new(&server.base_url(), "test-qdrant").unwrap();
    let spec = crate::testing::test_collection_spec_hybrid(2);
    qdrant_search(&http, &spec, &request).await.unwrap();
    mock.assert_async().await;
}

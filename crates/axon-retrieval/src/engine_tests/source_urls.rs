use super::*;

#[tokio::test]
async fn canonical_url_allowlist_filters_before_dense_and_hybrid_limits() {
    let store = Arc::new(FakeVectorStore::new("fake-vectors"));
    let spec = test_collection_spec_hybrid(4);
    store.ensure_collection(spec.clone()).await.unwrap();
    let allowed_uri = "https://docs.example.com/allowed";
    let mut allowed = point(
        "allowed",
        "allowed",
        &[-1.0, -1.0, -1.0, -1.0],
        "allowed evidence",
    );
    allowed.payload.get_mut("chunk_locator").unwrap()["canonical_uri"] = json!(allowed_uri);
    let mut denied = point(
        "denied",
        "denied",
        &[1.0, 1.0, 1.0, 1.0],
        "vector embedding split denied",
    );
    denied.payload.get_mut("chunk_locator").unwrap()["canonical_uri"] =
        json!("https://docs.example.com/allowed/extra");
    store
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(Uuid::from_u128(12)),
            collection: "axon-test".into(),
            model: "fake-embedding".into(),
            dimensions: 4,
            sparse_vectors: None,
            payload_indexes: spec.payload_indexes,
            points: vec![denied, allowed],
        })
        .await
        .unwrap();
    let engine = RetrievalEngine::new(
        store,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 4)),
        retrieval_config(),
    );
    for hybrid in [false, true] {
        let mut req = request();
        req.limit = 1;
        req.hybrid = hybrid;
        req.source_urls = vec![allowed_uri.into()];
        let result = engine.retrieve(req).await.unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].canonical_uri, allowed_uri);
        assert_eq!(result.citations.len(), 1);
        assert_eq!(result.citations[0].canonical_uri, allowed_uri);
        assert!(!result.context.text.contains("denied"));
    }
}

#[test]
fn canonical_url_allowlist_rejects_unbounded_or_malformed_values() {
    let mut plan = request().plan(RetrievalAccess::standard().allowed_visibility);
    for urls in [
        vec!["x".repeat(4097)],
        vec!["https://example.com".into(); 65],
        vec![String::new()],
        vec![" https://example.com".into()],
        vec!["https://example.com\n".into()],
    ] {
        plan.source_urls = urls;
        let error = crate::engine::search_filters(&plan).unwrap_err();
        assert_eq!(error.code.0, "retrieval.invalid_source_urls");
    }
    plan.source_urls = vec![
        "https://example.com/a".into(),
        "https://example.com/b".into(),
    ];
    let filters = crate::engine::search_filters(&plan).unwrap();
    assert_eq!(
        filters.get("chunk_locator.canonical_uri"),
        Some(&json!(plan.source_urls))
    );
}

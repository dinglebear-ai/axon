use super::*;

fn sample_hit(url: &str) -> QueryHit {
    QueryHit {
        rank: 1,
        score: 0.9,
        rerank_score: 0.9,
        url: url.to_string(),
        source: "fake".to_string(),
        snippet: "fake snippet".to_string(),
        chunk_index: Some(0),
        file_path: None,
        symbol: None,
        kind: None,
        start_line: None,
        end_line: None,
        file_type: None,
        language: None,
        provider: None,
        content_kind: None,
        chunking_method: None,
        symbol_extraction_status: None,
    }
}

#[tokio::test]
async fn fake_query_service_returns_seeded_hits() {
    let fake = FakeQueryService::new();
    fake.seed(sample_hit("https://example.com/a"));
    fake.seed(sample_hit("https://example.com/b"));

    let request = QueryRequest {
        query: Some("test".to_string()),
        limit: Some(1),
        offset: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    };
    let result = fake.query(request).await.expect("query should succeed");
    assert_eq!(result.results.len(), 1);
}

#[tokio::test]
async fn fake_query_service_works_through_trait_object() {
    let fake: Arc<dyn QueryService> = Arc::new(FakeQueryService::new());
    let request = QueryRequest {
        query: Some("test".to_string()),
        limit: Some(10),
        offset: None,
        collection: None,
        since: None,
        before: None,
        hybrid_search: None,
        response_mode: None,
    };
    let result = fake.query(request).await.expect("query should succeed");
    assert!(result.results.is_empty());
}

/// Compile-only check: `QueryServiceImpl` satisfies `QueryService`. Not
/// executed — constructing a real `ServiceContext` needs live services.
fn _assert_query_service_impl<T: QueryService>() {}
#[allow(dead_code)]
fn _query_service_impl_satisfies_trait() {
    _assert_query_service_impl::<QueryServiceImpl>();
}

use super::*;

fn citation(url: &str) -> axon_api::CanonicalCitation {
    axon_api::CanonicalCitation {
        source_id: axon_api::SourceId::new("source-test"),
        source_item_key: axon_api::SourceItemKey::new(url),
        generation: axon_api::SourceGenerationId::new("1"),
        document_id: axon_api::DocumentId::new("document-test"),
        chunk_id: axon_api::ChunkId::new("chunk-test"),
        job_id: axon_api::JobId::new(uuid::Uuid::from_u128(1)),
        canonical_uri: url.to_string(),
        source_range: axon_api::SourceRange {
            line_start: Some(1),
            line_end: Some(1),
            byte_start: None,
            byte_end: None,
            char_start: None,
            char_end: None,
            time_start_ms: None,
            time_end_ms: None,
            dom_selector: None,
            json_pointer: None,
            yaml_path: None,
            xml_xpath: None,
            csv_row: None,
            session_turn_id: None,
            turn_start: None,
            turn_end: None,
        },
        redaction: axon_api::RedactionMetadata {
            redaction_status: axon_api::RedactionStatus::Clean,
            redaction_version: "test-v1".to_string(),
            visibility: axon_api::Visibility::Public,
            redacted_field_count: 0,
            dropped_field_count: 0,
            detector_count: 0,
            detector_names: Vec::new(),
        },
    }
}

fn sample_hit(url: &str) -> QueryHit {
    QueryHit {
        rank: 1,
        score: 0.9,
        rerank_score: 0.9,
        url: url.to_string(),
        source: "fake".to_string(),
        snippet: "fake snippet".to_string(),
        citation: citation(url),
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

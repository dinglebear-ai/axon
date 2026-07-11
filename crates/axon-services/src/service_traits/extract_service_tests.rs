use std::sync::Arc;

use super::*;

/// Compile-level assertion that the production impl satisfies the trait
/// object (no live providers exercised).
#[allow(dead_code)]
fn assert_extract_service_impl_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn ExtractService> {
    Arc::new(ExtractServiceImpl::new(ctx))
}

#[tokio::test]
async fn fake_extract_service_extract_echoes_urls() {
    let fake = FakeExtractService::new();
    let request = ExtractRequest {
        subaction: None,
        urls: Some(vec!["https://example.com".to_string()]),
        prompt: None,
        max_pages: None,
        render_mode: None,
        embed: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let result: ExtractResult = fake.extract(request).await.expect("extract should succeed");
    assert_eq!(result.urls, vec!["https://example.com".to_string()]);
    assert!(result.extracted.is_empty());
    assert_eq!(fake.call_count(), 1);
}

#[tokio::test]
async fn fake_extract_service_extract_through_trait_object() {
    let fake: Arc<dyn ExtractService> = Arc::new(FakeExtractService::new());
    let request = ExtractRequest {
        subaction: None,
        urls: Some(vec!["https://example.org".to_string()]),
        prompt: None,
        max_pages: None,
        render_mode: None,
        embed: None,
        job_id: None,
        limit: None,
        offset: None,
        response_mode: None,
    };
    let result = fake.extract(request).await.expect("extract should succeed");
    assert_eq!(result.urls, vec!["https://example.org".to_string()]);
}

#[tokio::test]
async fn fake_extract_service_summarize_returns_fake_summary() {
    let fake = FakeExtractService::new();
    let request = SummarizeRequest {
        url: Some("https://example.com".to_string()),
        urls: None,
        render_mode: None,
        root_selector: None,
        exclude_selector: None,
        response_mode: None,
    };
    let result = fake
        .summarize(request)
        .await
        .expect("summarize should succeed");
    assert_eq!(result.urls, vec!["https://example.com".to_string()]);
    assert_eq!(fake.call_count(), 1);
}

#[tokio::test]
async fn fake_extract_service_research_returns_payload() {
    let fake = FakeExtractService::new();
    let request = ResearchRequest {
        query: Some("axon rag".to_string()),
        limit: Some(5),
        offset: None,
        search_time_range: None,
        response_mode: None,
    };
    let result = fake
        .research(request)
        .await
        .expect("research should succeed");
    assert_eq!(result.payload.query, "axon rag");
}

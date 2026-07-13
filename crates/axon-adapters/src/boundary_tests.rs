use axon_api::source::*;

use super::*;

#[tokio::test]
async fn fake_adapter_providers_cover_search_fetch_render_and_capture() {
    let fake = FakeAdapterProviders::new();
    assert_eq!(
        fake.search(SearchRequest {
            query: "axon".to_string(),
            limit: 1,
            offset: 0,
            time_range: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap()
        .results
        .len(),
        1
    );

    let fetched = fake
        .fetch(FetchRequest {
            uri: "https://example.test".to_string(),
            method: "GET".to_string(),
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            body: None,
            timeout_ms: Some(1000),
            max_bytes: Some(1024),
            credential_refs: Vec::new(),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert_eq!(fetched.status, 200);
    assert_eq!(fetched.final_uri, "https://example.test");
    assert_eq!(
        fetched.fetched_at,
        Timestamp("2026-07-01T00:00:00Z".to_string())
    );

    let rendered = fake
        .render(RenderRequest {
            uri: "https://example.test".to_string(),
            mode: RenderMode::Http,
            timeout_ms: Some(1000),
            wait_ms: None,
            automation_script: None,
            credential_refs: Vec::new(),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert_eq!(rendered.markdown, "fake render");
    assert_eq!(rendered.render_mode, RenderMode::Http);

    let captured = fake
        .capture(NetworkCaptureRequest {
            uri: "https://example.test".to_string(),
            include_request_headers: true,
            include_response_headers: true,
            include_bodies: false,
            timeout_ms: Some(1000),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert!(captured.entries.is_empty());
    assert_eq!(
        NetworkCaptureProvider::capabilities(&fake)
            .await
            .unwrap()
            .provider_kind,
        ProviderKind::NetworkCapture
    );
}

#[tokio::test]
async fn fake_adapter_providers_report_health_override() {
    let fake = FakeAdapterProviders::new().with_health(HealthStatus::Cooling);

    let capability = FetchProvider::capabilities(&fake).await.unwrap();

    assert_eq!(capability.health, HealthStatus::Cooling);
}

#[tokio::test]
async fn fake_adapter_provider_capabilities_reflect_failure_mode() {
    let timeout = FakeAdapterProviders::new().with_mode(FakeAdapterMode::Timeout);
    assert_eq!(
        FetchProvider::capabilities(&timeout).await.unwrap().health,
        HealthStatus::Degraded
    );

    let rate_limited = FakeAdapterProviders::new().with_mode(FakeAdapterMode::RateLimited);
    let capability = FetchProvider::capabilities(&rate_limited).await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.rate_limited"
    );

    let fake = FakeAdapterProviders::new().with_mode(FakeAdapterMode::Fatal);

    let capability = FetchProvider::capabilities(&fake).await.unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    let error = capability.last_error.unwrap();
    assert_eq!(error.code.to_string(), "provider.fatal");
    assert_eq!(error.provider_id, Some("fake_fetch".to_string()));
    assert!(!error.retryable);
}

#[tokio::test]
async fn fake_adapter_providers_return_failure_modes_and_record_calls() {
    let rate_limited = FakeAdapterProviders::new().with_mode(FakeAdapterMode::RateLimited);

    let err = rate_limited
        .search(SearchRequest {
            query: "axon".to_string(),
            limit: 1,
            offset: 0,
            time_range: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.rate_limited");
    assert!(err.retryable);
    assert_eq!(rate_limited.calls().await, vec!["search"]);

    let fatal = FakeAdapterProviders::new().with_mode(FakeAdapterMode::Fatal);
    let err = fatal
        .capture(NetworkCaptureRequest {
            uri: "https://example.test".to_string(),
            include_request_headers: true,
            include_response_headers: true,
            include_bodies: false,
            timeout_ms: Some(1000),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(err.code.to_string(), "provider.fatal");
    assert!(!err.retryable);
    assert_eq!(fatal.calls().await, vec!["capture"]);
}

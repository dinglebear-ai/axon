//! Real-client integration tests for [`HttpFetchProvider`] against a mock HTTP
//! server (httpmock). No live network is required.

use std::time::Duration;

use axon_api::source::*;
use httpmock::prelude::*;

use super::*;

fn request(uri: String) -> FetchRequest {
    FetchRequest {
        uri,
        method: "GET".to_string(),
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        body: None,
        timeout_ms: None,
        max_bytes: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn provider(timeout: Duration) -> HttpFetchProvider {
    HttpFetchProvider::new(HttpFetchConfig {
        timeout,
        max_bytes: None,
        user_agent: None,
    })
}

#[tokio::test]
async fn fetch_returns_body_status_and_etag_on_success() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/ok");
            then.status(200)
                .header("etag", "\"abc123\"")
                .header("content-type", "text/plain")
                .body("hello world");
        })
        .await;

    let provider = provider(Duration::from_secs(5));
    let resource = provider
        .fetch(request(format!("{}/ok", server.base_url())))
        .await
        .expect("fetch should succeed");

    assert_eq!(resource.status, 200);
    assert_eq!(resource.etag.as_deref(), Some("\"abc123\""));
    assert_eq!(resource.bytes, Some(11));
    match resource.content {
        ContentRef::InlineText { text } => assert_eq!(text, "hello world"),
        other => panic!("expected InlineText, got {other:?}"),
    }

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Healthy);
    assert!(capability.cooldown_until.is_none());
}

#[tokio::test]
async fn fetch_timeout_marks_provider_degraded() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/slow");
            then.status(200)
                .delay(Duration::from_millis(300))
                .body("too slow");
        })
        .await;

    // Client timeout (50ms) is far shorter than the mock's 300ms delay.
    let provider = provider(Duration::from_millis(50));
    let err = provider
        .fetch(request(format!("{}/slow", server.base_url())))
        .await
        .expect_err("a client-side timeout must surface as an error");
    assert_eq!(err.code.to_string(), "fetch.timeout");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Degraded);
    assert!(capability.cooldown_until.is_none());
}

#[tokio::test]
async fn fetch_rate_limited_cools_the_provider_with_cooldown_until() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rate-limited");
            then.status(429);
        })
        .await;

    let provider = provider(Duration::from_secs(5));
    let err = provider
        .fetch(request(format!("{}/rate-limited", server.base_url())))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(err.code.to_string(), "fetch.rate_limited");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(capability.reservation_state.available_units, 0);
}

#[tokio::test]
async fn fetch_server_error_marks_provider_unavailable() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/broken");
            then.status(503);
        })
        .await;

    let provider = provider(Duration::from_secs(5));
    let err = provider
        .fetch(request(format!("{}/broken", server.base_url())))
        .await
        .expect_err("5xx must surface as an error");
    assert_eq!(err.code.to_string(), "fetch.server_error");
    assert!(!err.retryable);

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Unavailable);
}

#[tokio::test]
async fn fetch_rejects_blocked_ssrf_targets_without_network() {
    let provider = provider(Duration::from_secs(5));
    let err = provider
        .fetch(request("http://127.0.0.1:1/".to_string()))
        .await
        .expect_err("loopback targets must be rejected before any request is sent");
    assert_eq!(err.code.to_string(), "fetch.invalid_uri");
}

#[tokio::test]
async fn a_successful_fetch_recovers_a_previously_cooling_provider() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rate-limited");
            then.status(429);
        })
        .await;

    let provider = provider(Duration::from_secs(5));
    provider
        .fetch(request(format!("{}/rate-limited", server.base_url())))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Cooling
    );

    server
        .mock_async(|when, then| {
            when.method(GET).path("/ok");
            then.status(200).body("ok");
        })
        .await;
    provider
        .fetch(request(format!("{}/ok", server.base_url())))
        .await
        .expect("a subsequent success clears cooldown");

    let recovered = provider.capabilities().await.expect("capabilities");
    assert_eq!(recovered.health, HealthStatus::Healthy);
    assert!(recovered.cooldown_until.is_none());
}

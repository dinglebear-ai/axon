//! Real-client integration tests for [`SearxngSearchProvider`] against a mock
//! HTTP server (httpmock). No live network is required.

use std::time::Duration;

use axon_api::source::*;
use httpmock::prelude::*;

use super::*;

fn request(query: &str, limit: u32) -> SearchRequest {
    SearchRequest {
        query: query.to_string(),
        limit,
        metadata: MetadataMap::new(),
    }
}

fn provider(base_url: String, timeout: Duration) -> SearxngSearchProvider {
    SearxngSearchProvider::new(SearxngSearchConfig { base_url, timeout })
}

fn searx_json(results: &[(&str, &str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "results": results
            .iter()
            .map(|(url, title, content)| {
                serde_json::json!({ "url": url, "title": title, "content": content })
            })
            .collect::<Vec<_>>()
    })
}

#[tokio::test]
async fn search_returns_normalized_hits_on_success() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search").query_param("pageno", "1");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(searx_json(&[("https://example.test/a", "A", "snippet a")]));
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_secs(5));
    let result = provider
        .search(request("axon", 1))
        .await
        .expect("search should succeed");

    assert_eq!(result.query, "axon");
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].url, "https://example.test/a");
    assert_eq!(result.results[0].title, "A");
    assert_eq!(result.results[0].snippet, "snippet a");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Healthy);
    assert!(capability.cooldown_until.is_none());
}

#[tokio::test]
async fn search_dedupes_and_paginates_until_limit_satisfied() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search").query_param("pageno", "1");
            then.status(200).json_body(searx_json(&[
                ("https://a.test/1", "1", "c1"),
                ("https://a.test/2", "2", "c2"),
            ]));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search").query_param("pageno", "2");
            then.status(200).json_body(searx_json(&[
                ("https://a.test/1", "dup", "dup"), // duplicate URL, must be skipped
                ("https://a.test/3", "3", "c3"),
            ]));
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_secs(5));
    // limit=3 forces a walk into page 2 (page 1 yields only 2 unique hits).
    let result = provider
        .search(request("axon", 3))
        .await
        .expect("search should succeed");
    assert_eq!(result.results.len(), 3);
    assert_eq!(result.results[2].url, "https://a.test/3");
}

#[tokio::test]
async fn search_timeout_marks_provider_degraded() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(200)
                .delay(Duration::from_millis(300))
                .json_body(searx_json(&[]));
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_millis(50));
    let err = provider
        .search(request("axon", 1))
        .await
        .expect_err("a client-side timeout must surface as an error");
    assert_eq!(err.code.to_string(), "search.timeout");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Degraded);
    assert!(capability.cooldown_until.is_none());
}

#[tokio::test]
async fn search_rate_limited_cools_the_provider_with_cooldown_until() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(429);
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_secs(5));
    let err = provider
        .search(request("axon", 1))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(err.code.to_string(), "search.rate_limited");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(capability.reservation_state.available_units, 0);
}

#[tokio::test]
async fn search_disabled_json_format_marks_provider_unavailable() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(403);
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_secs(5));
    let err = provider
        .search(request("axon", 1))
        .await
        .expect_err("403 must surface as an error");
    assert_eq!(err.code.to_string(), "search.bad_status");
    assert!(!err.retryable);

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Unavailable);
}

#[tokio::test]
async fn search_rejects_blocked_ssrf_targets_without_network() {
    let provider = provider("http://127.0.0.1:1".to_string(), Duration::from_secs(5));
    let err = provider
        .search(request("axon", 1))
        .await
        .expect_err("loopback targets must be rejected before any request is sent");
    assert_eq!(err.code.to_string(), "search.invalid_url");
}

#[tokio::test]
async fn a_successful_search_recovers_a_previously_cooling_provider() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/search")
                .query_param("q", "axon-cooldown");
            then.status(429);
        })
        .await;

    let provider = provider(server.base_url(), Duration::from_secs(5));
    provider
        .search(request("axon-cooldown", 1))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(
        provider.capabilities().await.unwrap().health,
        HealthStatus::Cooling
    );

    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/search")
                .query_param("q", "axon-recover");
            then.status(200)
                .json_body(searx_json(&[("https://example.test/a", "A", "a")]));
        })
        .await;
    provider
        .search(request("axon-recover", 1))
        .await
        .expect("a subsequent success clears cooldown");

    let recovered = provider.capabilities().await.expect("capabilities");
    assert_eq!(recovered.health, HealthStatus::Healthy);
    assert!(recovered.cooldown_until.is_none());
}

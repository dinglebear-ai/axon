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

#[test]
fn default_fetch_provider_has_a_finite_response_limit() {
    let config = HttpFetchConfig::default();
    assert!(
        config.max_bytes.is_some_and(|limit| limit > 0),
        "network acquisition must be bounded even when callers omit max_bytes"
    );
}

#[test]
fn explicit_unlimited_config_still_has_a_hard_transport_ceiling() {
    let provider = provider(Duration::from_secs(1));
    assert_eq!(
        provider.effective_max_bytes(&request("https://example.com".into())),
        DEFAULT_MAX_RESPONSE_BYTES
    );
}

#[test]
fn credentials_may_follow_only_same_origin_non_downgrade_redirects() {
    let original = reqwest::Url::parse("https://example.com/private").unwrap();
    assert!(redirect_can_forward_credentials(
        &original,
        &reqwest::Url::parse("https://example.com/next").unwrap()
    ));
    assert!(!redirect_can_forward_credentials(
        &original,
        &reqwest::Url::parse("https://attacker.example/next").unwrap()
    ));
    assert!(!redirect_can_forward_credentials(
        &original,
        &reqwest::Url::parse("http://example.com/next").unwrap()
    ));
    assert!(!redirect_can_forward_credentials(
        &original,
        &reqwest::Url::parse("https://example.com:8443/next").unwrap()
    ));
}

#[test]
fn arbitrary_caller_headers_are_treated_as_credentials_for_redirects() {
    let mut request = request("https://example.com/private".to_string());
    request.headers.headers.push(RedactedHeader {
        name: "x-api-key".to_string(),
        value: "secret".to_string(),
        redacted: true,
    });
    assert!(request_carries_credentials(&request));
}

#[tokio::test]
async fn fetch_stops_reading_as_soon_as_stream_exceeds_limit() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let _loopback = axon_core::http::LoopbackGuard::allow();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (_finish_tx, finish_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\n0123456789\r\n")
            .await
            .unwrap();
        stream.flush().await.unwrap();
        let _ = finish_rx.await;
        let _ = stream.write_all(b"0\r\n\r\n").await;
    });

    let provider = provider(Duration::from_secs(5));
    let mut request = request(format!("http://{address}/large"));
    request.max_bytes = Some(5);
    let error = provider
        .fetch(request)
        .await
        .expect_err("body must be rejected");

    assert_eq!(error.code.to_string(), "fetch.response_too_large");
    server.abort();
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
async fn fetch_keeps_valid_utf8_pdf_bytes_out_of_text_content() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/agenda.pdf");
            then.status(200)
                .header("content-type", "application/pdf; version=1.7")
                .body(b"%PDF-1.7\n\0valid-utf8-binary");
        })
        .await;

    let provider = provider(Duration::from_secs(5));
    let resource = provider
        .fetch(request(format!("{}/agenda.pdf", server.base_url())))
        .await
        .expect("PDF fetch should succeed");

    match resource.content {
        ContentRef::InlineBytes {
            bytes_base64,
            mime_type,
        } => {
            assert!(!bytes_base64.is_empty());
            assert_eq!(mime_type, "application/pdf; version=1.7");
        }
        other => panic!("expected InlineBytes for PDF, got {other:?}"),
    }
}

#[test]
fn binary_content_type_detection_is_case_and_parameter_insensitive() {
    assert!(content_type_requires_binary(
        "Application/PDF; charset=binary"
    ));
    assert!(!content_type_requires_binary("image/svg+xml"));
    assert!(!content_type_requires_binary("text/html; charset=utf-8"));
    assert!(!content_type_requires_binary("application/json"));
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

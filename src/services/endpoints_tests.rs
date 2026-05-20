use super::*;
use crate::core::http::set_allow_loopback;
use crate::services::types::EndpointSourceKind;
use httpmock::Method::{HEAD, OPTIONS};
use httpmock::prelude::*;

#[tokio::test]
async fn service_fetches_page_and_first_party_bundles() {
    set_allow_loopback(true);
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .body(r#"<script src="/app.js"></script><script>fetch("/api/inline")</script>"#);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/app.js");
            then.status(200)
                .header("content-type", "application/javascript")
                .body(r#"fetch("/api/from-bundle")"#);
        })
        .await;

    let report = discover(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions::default(),
    )
    .await
    .expect("endpoint discovery");

    assert_eq!(report.bundles_scanned, 1);
    assert!(report.endpoints.iter().any(|e| e.value == "/api/inline"));
    assert!(
        report
            .endpoints
            .iter()
            .any(|e| e.value == "/api/from-bundle")
    );
    set_allow_loopback(false);
}

#[tokio::test]
async fn service_filters_first_party_only() {
    set_allow_loopback(true);
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200).body(
                r#"<script>fetch("/api/local"); fetch("https://thirdparty.example/api/remote")</script>"#,
            );
        })
        .await;

    let report = discover(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            first_party_only: true,
            include_bundles: false,
            ..EndpointOptions::default()
        },
    )
    .await
    .expect("endpoint discovery");

    assert!(report.endpoints.iter().all(|endpoint| endpoint.first_party));
    assert!(
        report
            .endpoints
            .iter()
            .any(|endpoint| endpoint.value == "/api/local")
    );
    set_allow_loopback(false);
}

#[tokio::test]
async fn verification_reports_head_405_options_fallback() {
    set_allow_loopback(true);
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .body(r#"<script>fetch("/api/probe")</script>"#);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(HEAD).path("/api/probe");
            then.status(405);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(OPTIONS).path("/api/probe");
            then.status(204).header("content-type", "application/json");
        })
        .await;

    let report = discover(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            verify: true,
            ..EndpointOptions::default()
        },
    )
    .await
    .expect("endpoint discovery");

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.value == "/api/probe")
        .expect("probe endpoint");
    let verified = endpoint.verified.as_ref().expect("verification");
    assert_eq!(verified.method, "OPTIONS");
    assert_eq!(verified.status, Some(204));
    set_allow_loopback(false);
}

#[tokio::test]
async fn fake_network_capture_merges_observed_requests() {
    struct FakeCapture;

    impl NetworkCaptureProvider for FakeCapture {
        async fn capture(
            &self,
            _cfg: &Config,
            url: &str,
            _max_requests: usize,
        ) -> Result<Vec<CapturedRequest>, EndpointError> {
            let captured_url = format!("{}/api/observed", url.trim_end_matches('/'));
            Ok(vec![CapturedRequest {
                url: captured_url,
                method: Some("GET".to_string()),
            }])
        }
    }

    set_allow_loopback(true);
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200).body("<html></html>");
        })
        .await;

    let report = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            ..EndpointOptions::default()
        },
        &FakeCapture,
    )
    .await
    .expect("capture discovery");

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.source == EndpointSourceKind::NetworkCapture)
        .expect("network endpoint");
    assert!(endpoint.value.ends_with("/api/observed"));
    set_allow_loopback(false);
}

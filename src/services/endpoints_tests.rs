use super::*;
use crate::core::http::{get_allow_loopback, set_allow_loopback};
use crate::services::types::{EndpointKind, EndpointSourceKind};
use httpmock::Method::{HEAD, OPTIONS};
use httpmock::prelude::*;
use serial_test::serial;

struct LoopbackGuard {
    previous: bool,
}

impl LoopbackGuard {
    fn allow() -> Self {
        let previous = get_allow_loopback();
        set_allow_loopback(true);
        Self { previous }
    }
}

impl Drop for LoopbackGuard {
    fn drop(&mut self) {
        set_allow_loopback(self.previous);
    }
}

#[tokio::test]
#[serial]
async fn service_fetches_page_and_first_party_bundles() {
    let _loopback = LoopbackGuard::allow();
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
        None,
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
}

#[tokio::test]
#[serial]
async fn service_filters_first_party_only() {
    let _loopback = LoopbackGuard::allow();
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
        None,
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
}

#[tokio::test]
#[serial]
async fn verification_reports_head_405_options_fallback() {
    let _loopback = LoopbackGuard::allow();
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
        None,
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
}

#[tokio::test]
#[serial]
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

    let _loopback = LoopbackGuard::allow();
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
        None,
    )
    .await
    .expect("capture discovery");

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.source == EndpointSourceKind::NetworkCapture)
        .expect("network endpoint");
    assert!(endpoint.value.ends_with("/api/observed"));
}

#[tokio::test]
#[serial]
async fn fake_network_capture_keeps_third_party_unless_filtered() {
    struct FakeCapture {
        third_party_url: String,
    }

    impl NetworkCaptureProvider for FakeCapture {
        async fn capture(
            &self,
            _cfg: &Config,
            url: &str,
            _max_requests: usize,
        ) -> Result<Vec<CapturedRequest>, EndpointError> {
            let captured_url = format!("{}/api/observed", url.trim_end_matches('/'));
            Ok(vec![
                CapturedRequest {
                    url: captured_url,
                    method: Some("GET".to_string()),
                },
                CapturedRequest {
                    url: self.third_party_url.clone(),
                    method: Some("POST".to_string()),
                },
            ])
        }
    }

    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200).body("<html></html>");
        })
        .await;
    let third_party_url = format!("http://127.0.0.2:{}/api/third", server.port());

    let report = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            ..EndpointOptions::default()
        },
        &FakeCapture {
            third_party_url: third_party_url.clone(),
        },
        None,
    )
    .await
    .expect("capture discovery");

    assert!(
        report
            .endpoints
            .iter()
            .any(|endpoint| endpoint.normalized_url.as_deref() == Some(third_party_url.as_str()))
    );

    let filtered = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            first_party_only: true,
            ..EndpointOptions::default()
        },
        &FakeCapture { third_party_url },
        None,
    )
    .await
    .expect("filtered capture discovery");

    assert!(!filtered.endpoints.is_empty());
    assert!(
        filtered
            .endpoints
            .iter()
            .all(|endpoint| endpoint.first_party)
            && filtered
                .endpoints
                .iter()
                .any(|endpoint| endpoint.value.ends_with("/api/observed"))
    );
}

#[tokio::test]
#[serial]
async fn fake_network_capture_merges_websocket_requests() {
    struct FakeCapture {
        websocket_url: String,
    }

    impl NetworkCaptureProvider for FakeCapture {
        async fn capture(
            &self,
            _cfg: &Config,
            _url: &str,
            _max_requests: usize,
        ) -> Result<Vec<CapturedRequest>, EndpointError> {
            Ok(vec![CapturedRequest {
                url: self.websocket_url.clone(),
                method: Some("GET".to_string()),
            }])
        }
    }

    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200).body("<html></html>");
        })
        .await;
    let websocket_url = format!("ws://127.0.0.2:{}/api/socket", server.port());

    let report = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            ..EndpointOptions::default()
        },
        &FakeCapture {
            websocket_url: websocket_url.clone(),
        },
        None,
    )
    .await
    .expect("websocket capture discovery");

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.normalized_url.as_deref() == Some(websocket_url.as_str()))
        .expect("websocket endpoint");
    assert_eq!(endpoint.kind, EndpointKind::Websocket);
}

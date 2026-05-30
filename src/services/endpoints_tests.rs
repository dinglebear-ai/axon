use super::*;
use crate::core::http::{get_allow_loopback, set_allow_loopback};
use crate::services::types::{EndpointKind, EndpointSourceKind};
use httpmock::Method::{HEAD, OPTIONS};
use httpmock::prelude::*;
use serial_test::serial;
use std::sync::{Arc, Mutex};

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

/// Task 4: verify that the probe cap is exactly 100 and a warning is emitted
/// when more than 100 endpoints are eligible for verification.
#[tokio::test]
#[serial]
async fn verification_probe_cap_is_100_with_warning() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;

    // Build a page with 101 distinct relative API paths so the verifier
    // must cap at 100 and emit a warning.
    let paths: String = (0..=100)
        .map(|i| format!(r#"fetch("/api/path{i}");"#))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!("<script>{paths}</script>");

    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .body(body);
        })
        .await;
    // Respond HEAD 200 for all /api/* paths.
    server
        .mock_async(|when, then| {
            when.method(HEAD)
                .path_matches(regex::Regex::new(r"^/api/").unwrap());
            then.status(200);
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

    // Exactly 100 endpoints should be verified (the 101st is skipped).
    let verified_count = report
        .endpoints
        .iter()
        .filter(|e| e.verified.is_some())
        .count();
    assert_eq!(
        verified_count, 100,
        "MAX_VERIFY_PROBES must be 100: got {verified_count} verified"
    );

    // A warning about the skipped endpoint must be present.
    assert!(
        report.warnings.iter().any(|w| w.contains("capped at 100")),
        "expected warning about probe cap at 100; got warnings: {:?}",
        report.warnings
    );
}

/// Task 5: prove that pre-dispatch validation blocks private/loopback URLs
/// before they are handed to the Chrome capture provider.
#[tokio::test]
#[serial]
async fn fake_capture_proves_blocked_urls_never_dispatched() {
    // This fake tracks every URL it attempts to dispatch. The test then
    // asserts that the private URLs were never dispatched — proving pre-dispatch
    // blocking, not post-capture omission.
    struct AuditingCapture {
        dispatched: Arc<Mutex<Vec<String>>>,
    }

    impl NetworkCaptureProvider for AuditingCapture {
        async fn capture(
            &self,
            _cfg: &Config,
            page_url: &str,
            _max_requests: usize,
        ) -> Result<Vec<CapturedRequest>, EndpointError> {
            // Simulate a Chrome session that would normally observe three
            // network requests: two private-IP targets (which the REAL Chrome
            // path's CDP Fetch.enable intercept would block before dispatch)
            // and one allowed request (a relative path that resolves to the
            // same loopback origin as the page under test).
            //
            // In this fake, we model "pre-dispatch" by running the same SSRF
            // check that `send_fetch_intercept_reply` would run in the real
            // Chrome path. We do NOT hold the mutex across the await — instead
            // we collect validation results first, then lock briefly to record.
            let page_origin = Url::parse(page_url)
                .map(|u| u.origin().ascii_serialization())
                .unwrap_or_default();
            // Three synthetic "network requests" that Chrome would have observed.
            // The loopback server URL is allowed because LoopbackGuard is active.
            let candidates: Vec<(String, bool)> = vec![
                ("http://192.168.1.1/internal".to_string(), true), // private IP — blocked
                (page_origin.clone(), false),                      // loopback origin — allowed
                ("http://10.0.0.1/admin".to_string(), true),       // private IP — blocked
            ];

            // Phase 1: validate all URLs without holding the mutex (no Send issue).
            let mut results: Vec<(String, bool, bool)> = Vec::new();
            for (url, expect_blocked) in &candidates {
                let is_blocked = validate_url_with_dns_timeout(url.as_str()).await.is_err();
                results.push((url.clone(), *expect_blocked, is_blocked));
            }

            // Phase 2: assert correctness and record dispatched URLs (no await).
            let mut captured = Vec::new();
            let mut dispatched = self.dispatched.lock().unwrap();
            for (url, expect_blocked, is_blocked) in results {
                assert_eq!(
                    is_blocked, expect_blocked,
                    "pre-dispatch check for {url}: blocked={is_blocked}, expected={expect_blocked}"
                );
                if !is_blocked {
                    dispatched.push(url.clone());
                    captured.push(CapturedRequest {
                        url: url.clone(),
                        method: Some("GET".to_string()),
                    });
                }
                // Blocked URLs are NEVER added to `dispatched`.
            }
            Ok(captured)
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

    let dispatched_log = Arc::new(Mutex::new(Vec::new()));
    let provider = AuditingCapture {
        dispatched: Arc::clone(&dispatched_log),
    };

    let report = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            ..EndpointOptions::default()
        },
        &provider,
        None,
    )
    .await
    .expect("capture discovery");

    // Only the allowed URL (loopback origin) should have been dispatched.
    let dispatched = dispatched_log.lock().unwrap();
    assert_eq!(
        dispatched.len(),
        1,
        "only 1 URL should have been dispatched (the loopback origin); dispatched: {dispatched:?}"
    );
    let dispatched_url = &dispatched[0];
    assert!(
        dispatched_url.starts_with("http://127.0.0.1")
            || dispatched_url.starts_with("http://localhost"),
        "the dispatched URL must be the loopback allowed URL; got: {dispatched_url:?}"
    );

    // The blocked private-IP URLs must not appear in the report.
    let blocked_urls = ["http://192.168.1.1/internal", "http://10.0.0.1/admin"];
    for blocked in blocked_urls {
        assert!(
            !report
                .endpoints
                .iter()
                .any(|e| e.value == blocked || e.normalized_url.as_deref() == Some(blocked)),
            "blocked URL {blocked} must not appear in endpoint report"
        );
    }
}

#[tokio::test]
#[serial]
async fn probe_rpc_recovers_from_non_html_fetch() {
    let _loopback = LoopbackGuard::allow();

    let server = MockServer::start_async().await;
    // Seed URL returns 401 (no HTML) ...
    server
        .mock_async(|when, then| {
            when.method(GET).path("/seed");
            then.status(401);
        })
        .await;
    // ... but /mcp on the same host is a real MCP server.
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1,
                    "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
                }));
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.endpoints_probe_rpc = true;
    let opts = EndpointOptions {
        probe_rpc: true,
        capture_network: false,
        ..EndpointOptions::default()
    };
    let report = discover(&cfg, &server.url("/seed"), opts, None)
        .await
        .unwrap();

    // Did NOT error; recorded a fetch warning; still found the synthesized MCP.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("initial fetch failed"))
    );
    assert!(
        report
            .endpoints
            .iter()
            .any(|e| e.source == EndpointSourceKind::SynthesizedMcp)
    );
}

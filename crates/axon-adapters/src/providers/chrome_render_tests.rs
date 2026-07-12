//! Tests for [`ChromeRenderProvider`].
//!
//! No live Chrome/CDP endpoint is available in this environment, so coverage
//! splits in two:
//! - `RenderMode::Http` (and the default `AutoSwitch`, which also stays on
//!   the HTTP path for a single-page render — see
//!   `axon_crawl::scrape::fetch_single_page`) is exercised end-to-end against
//!   an httpmock server, including the error→capability classification wired
//!   through `render()`.
//! - request-mapping and error-classification pure functions are tested
//!   directly.
//! - anything that requires an actual `RenderMode::Chrome` browser is marked
//!   `#[ignore]` with the reason documented on the test.

use axon_api::source::*;
use httpmock::prelude::*;

use super::*;

fn request(uri: String, mode: RenderMode) -> RenderRequest {
    RenderRequest {
        uri,
        mode,
        timeout_ms: None,
        wait_ms: None,
        automation_script: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn provider() -> ChromeRenderProvider {
    ChromeRenderProvider::new(ChromeRenderConfig::default())
}

#[test]
fn map_render_mode_round_trips_all_variants() {
    for mode in [RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch] {
        assert_eq!(map_core_render_mode(map_render_mode(mode)), mode);
    }
}

#[test]
fn classify_render_error_recognizes_timeout() {
    assert_eq!(
        classify_render_error("fetch failed for scrape of https://x/: operation timed out"),
        RenderFailureClass::Timeout
    );
    assert_eq!(
        classify_render_error("request TIMEOUT while fetching"),
        RenderFailureClass::Timeout
    );
}

#[test]
fn classify_render_error_recognizes_rate_limiting() {
    assert_eq!(
        classify_render_error("scrape failed: HTTP 429 for https://x/"),
        RenderFailureClass::RateLimited
    );
    assert_eq!(
        classify_render_error("provider rate limit exceeded"),
        RenderFailureClass::RateLimited
    );
}

#[test]
fn classify_render_error_defaults_unmatched_errors_to_fatal() {
    assert_eq!(
        classify_render_error("scrape failed: HTTP 503 for https://x/"),
        RenderFailureClass::Fatal
    );
    assert_eq!(
        classify_render_error("connection refused"),
        RenderFailureClass::Fatal
    );
}

fn automation_script_ref(uri: &str) -> ArtifactRef {
    ArtifactRef {
        artifact_id: ArtifactId::new("art_1"),
        artifact_kind: ArtifactKind::RawContent,
        uri: uri.to_string(),
        size_bytes: None,
        content_hash: None,
        created_at: Timestamp::from(chrono::Utc::now()),
    }
}

#[test]
fn automation_script_path_strips_file_scheme() {
    assert_eq!(
        automation_script_path("file:///tmp/script.json"),
        std::path::PathBuf::from("/tmp/script.json")
    );
    assert_eq!(
        automation_script_path("/tmp/script.json"),
        std::path::PathBuf::from("/tmp/script.json")
    );
}

/// Regression 1 restoration (issue #298 Wave 2b): an automation script is no
/// longer unconditionally rejected. On an `Http`-mode render (no Chrome
/// involved), `web_engine::scrape::apply_automation_scripts` skips loading it
/// entirely (with a warning) rather than erroring — proven here by pointing
/// `automation_script` at a path that does not exist on disk: if the Http
/// path attempted to load it, this render would fail with an I/O error
/// instead of succeeding.
#[tokio::test]
async fn render_http_mode_skips_automation_script_with_warning() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><body><p>hello</p></body></html>");
        })
        .await;

    let provider = provider();
    let url = format!("{}/page", server.base_url());
    let mut req = request(url, RenderMode::Http);
    req.automation_script = Some(automation_script_ref("/nonexistent/script.json"));

    let rendered = provider
        .render(req)
        .await
        .expect("http-mode render must succeed even with automation_script set");
    assert!(rendered.markdown.contains("hello"));
}

#[tokio::test]
async fn render_http_mode_returns_markdown_and_html() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200).header("content-type", "text/html").body(
                "<html><head><title>Hi</title></head><body><p>hello render</p></body></html>",
            );
        })
        .await;

    let provider = provider();
    let url = format!("{}/page", server.base_url());
    let rendered = provider
        .render(request(url.clone(), RenderMode::Http))
        .await
        .expect("render should succeed over HTTP");

    assert_eq!(rendered.render_mode, RenderMode::Http);
    assert!(rendered.markdown.contains("hello render"));
    assert!(
        rendered
            .html
            .as_deref()
            .expect("html must be populated")
            .contains("<p>hello render</p>")
    );

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Healthy);
}

#[tokio::test]
async fn render_server_error_marks_provider_unavailable() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/broken");
            then.status(503);
        })
        .await;

    let provider = provider();
    let url = format!("{}/broken", server.base_url());
    let err = provider
        .render(request(url, RenderMode::Http))
        .await
        .expect_err("5xx must surface as an error");
    assert_eq!(err.code.to_string(), "render.fatal");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Unavailable);
}

#[tokio::test]
async fn render_rate_limited_cools_the_provider_with_cooldown_until() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rate-limited");
            then.status(429);
        })
        .await;

    let provider = provider();
    let url = format!("{}/rate-limited", server.base_url());
    let err = provider
        .render(request(url, RenderMode::Http))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(err.code.to_string(), "render.rate_limited");

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
}

/// Requires a live Chrome instance reachable over CDP
/// (`AXON_CHROME_REMOTE_URL`), which is not available in this sandbox — the
/// `chrome_runtime_requested`/`bootstrap_chrome_runtime` probe would either
/// hang waiting on a real browser or fall back to Spider's local Chrome
/// launcher, neither of which is deterministic in CI. Left as a documented
/// manual smoke test for an environment with Chrome configured.
#[tokio::test]
#[ignore = "requires a live Chrome/CDP endpoint, not available in this sandbox"]
async fn render_chrome_mode_against_a_live_browser() {
    let provider = ChromeRenderProvider::new(ChromeRenderConfig {
        chrome_remote_url: std::env::var("AXON_CHROME_REMOTE_URL").ok(),
        default_timeout_ms: Some(10_000),
    });
    let rendered = provider
        .render(request(
            "https://example.com/".to_string(),
            RenderMode::Chrome,
        ))
        .await
        .expect("render should succeed against a live Chrome instance");
    assert!(!rendered.markdown.is_empty());
}

/// Same live-Chrome requirement as `render_chrome_mode_against_a_live_browser`,
/// plus a real automation-script file on disk. Manual smoke test for
/// regression 1 (issue #298 Wave 2b) end-to-end: `automation_script` should
/// execute against the rendered page rather than being rejected or silently
/// skipped.
#[tokio::test]
#[ignore = "requires a live Chrome/CDP endpoint, not available in this sandbox"]
async fn render_chrome_mode_runs_automation_script_against_a_live_browser() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script_path = dir.path().join("automation.json");
    std::fs::write(&script_path, r#"{"/": [{"action": "wait", "ms": 100}]}"#)
        .expect("write automation script");

    let provider = ChromeRenderProvider::new(ChromeRenderConfig {
        chrome_remote_url: std::env::var("AXON_CHROME_REMOTE_URL").ok(),
        default_timeout_ms: Some(10_000),
    });
    let mut req = request("https://example.com/".to_string(), RenderMode::Chrome);
    req.automation_script = Some(automation_script_ref(&script_path.to_string_lossy()));

    let rendered = provider
        .render(req)
        .await
        .expect("render with automation_script should succeed against a live Chrome instance");
    assert!(!rendered.markdown.is_empty());
}

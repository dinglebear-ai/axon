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

#[tokio::test]
async fn render_rejects_automation_scripts_without_network() {
    let provider = provider();
    let mut req = request("https://example.test/".to_string(), RenderMode::Http);
    req.automation_script = Some(ArtifactRef {
        artifact_id: ArtifactId::new("art_1"),
        artifact_kind: ArtifactKind::Manifest,
        uri: "file:///tmp/script.json".to_string(),
        size_bytes: None,
        content_hash: None,
        created_at: Timestamp::from(chrono::Utc::now()),
    });

    let err = provider
        .render(req)
        .await
        .expect_err("automation scripts are unsupported in Wave 1a");
    assert_eq!(err.code.to_string(), "render.automation_script_unsupported");
}

#[tokio::test]
async fn render_http_mode_returns_markdown_and_html() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><head><title>Hi</title></head><body><p>hello render</p></body></html>");
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

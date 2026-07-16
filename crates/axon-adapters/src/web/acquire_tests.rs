use axon_api::source::*;
use httpmock::prelude::*;

use crate::boundary::FakeAdapterProviders;
use crate::providers::http_fetch::{HttpFetchConfig, HttpFetchProvider};

use super::*;

fn item(uri: &str) -> ManifestItem {
    ManifestItem {
        source_id: SourceId::from("src_web_acquire_test"),
        source_item_key: SourceItemKey::from("docs/intro"),
        canonical_uri: uri.to_string(),
        item_kind: ItemKind::WebPage,
        content_kind: None,
        display_path: Some("docs/intro".to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }
}

fn item_with_etag(uri: &str, etag: &str) -> ManifestItem {
    let mut i = item(uri);
    i.metadata
        .insert("web_etag".to_string(), serde_json::json!(etag));
    i
}

fn item_with_current_and_prior_etags(
    uri: &str,
    current_etag: &str,
    prior_etag: &str,
) -> ManifestItem {
    let mut i = item_with_etag(uri, current_etag);
    i.metadata
        .insert("web_prior_etag".to_string(), serde_json::json!(prior_etag));
    i
}

fn opts(mode: RenderMode, min_markdown_chars: usize) -> AcquireOptions {
    AcquireOptions {
        job_id: JobId::new(uuid::Uuid::nil()),
        mode,
        min_markdown_chars,
        automation_script: None,
        custom_headers: Vec::new(),
        etag_conditional: false,
        vertical: VerticalOptions {
            enabled: false,
            auto_dispatch_skip: Vec::new(),
            user_agent: None,
        },
    }
}

fn require_item(outcome: AcquiredItem, message: &str) -> AcquiredSourceItem {
    assert!(outcome.warnings.is_empty(), "unexpected warning");
    outcome.item.expect(message)
}

#[tokio::test]
async fn http_mode_calls_fetch_only_and_defaults_content_kind_to_html() {
    let providers = FakeAdapterProviders::new();
    let acquired = require_item(
        acquire_item(
            &providers,
            &providers,
            &item("https://example.com/docs/intro"),
            &opts(RenderMode::Http, 200),
        )
        .await
        .unwrap(),
        "http fetch should not be skipped",
    );

    assert_eq!(providers.calls().await, vec!["fetch"]);
    assert_eq!(acquired.manifest_item.content_kind, Some(ContentKind::Html));
    assert!(matches!(
        acquired.content_ref,
        ContentRef::InlineText { .. }
    ));
    assert_eq!(acquired.metadata["web_render_mode"], "http");
}

#[tokio::test]
async fn chrome_mode_calls_render_once() {
    let providers = FakeAdapterProviders::new();
    let acquired = require_item(
        acquire_item(
            &providers,
            &providers,
            &item("https://example.com/docs/intro"),
            &opts(RenderMode::Chrome, 200),
        )
        .await
        .unwrap(),
        "chrome render should not be skipped",
    );

    assert_eq!(providers.calls().await, vec!["render"]);
    assert_eq!(
        acquired.manifest_item.content_kind,
        Some(ContentKind::Markdown)
    );
    assert_eq!(acquired.metadata["web_fetch_method"], "chrome_render");
}

#[tokio::test]
async fn auto_switch_keeps_single_render_when_not_thin() {
    let providers = FakeAdapterProviders::new();
    // The fake's fixed "fake render" body (11 chars) is not thin against a
    // low threshold, so no Chrome re-render should occur.
    let acquired = require_item(
        acquire_item(
            &providers,
            &providers,
            &item("https://example.com/docs/intro"),
            &opts(RenderMode::AutoSwitch, 5),
        )
        .await
        .unwrap(),
        "auto-switch should not be skipped",
    );

    assert_eq!(providers.calls().await, vec!["render"]);
    assert_eq!(acquired.metadata["web_fetch_method"], "auto_switch_http");
}

#[tokio::test]
async fn auto_switch_re_renders_with_chrome_when_thin() {
    let providers = FakeAdapterProviders::new();
    // The fake's fixed "fake render" body (11 chars) is thin against a high
    // threshold, so a second (Chrome) render must occur.
    let acquired = require_item(
        acquire_item(
            &providers,
            &providers,
            &item("https://example.com/docs/intro"),
            &opts(RenderMode::AutoSwitch, 1000),
        )
        .await
        .unwrap(),
        "auto-switch should not be skipped",
    );

    assert_eq!(providers.calls().await, vec!["render", "render"]);
    assert_eq!(acquired.metadata["web_fetch_method"], "auto_switch_chrome");
}

#[tokio::test]
async fn http_mode_propagates_fetch_errors() {
    let providers = FakeAdapterProviders::new().with_mode(crate::boundary::FakeAdapterMode::Fatal);
    let err = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &opts(RenderMode::Http, 200),
    )
    .await
    .unwrap_err();

    assert!(!err.code.to_string().is_empty());
}

#[tokio::test]
async fn all_web_modes_reject_blocked_targets_before_provider_dispatch() {
    for mode in [RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch] {
        for uri in [
            "http://127.0.0.1/admin",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.2/private",
            "http://[fe80::1]/private",
            "file:///etc/passwd",
        ] {
            let providers = FakeAdapterProviders::new();
            let err = acquire_item(&providers, &providers, &item(uri), &opts(mode, 200))
                .await
                .expect_err("blocked target must fail before provider dispatch");
            assert_eq!(err.code.to_string(), "web.acquire.invalid_uri");
            assert!(
                providers.calls().await.is_empty(),
                "{mode:?} dispatched a provider for blocked target {uri}"
            );
        }
    }
}

// ── Regression 1: automation_script threading ───────────────────────────────

fn automation_ref(uri: &str) -> ArtifactRef {
    ArtifactRef {
        artifact_id: ArtifactId::new("art_automation"),
        artifact_kind: ArtifactKind::RawContent,
        uri: uri.to_string(),
        size_bytes: None,
        content_hash: None,
        created_at: super::super::timestamp(),
    }
}

#[test]
fn build_render_request_threads_automation_script() {
    let req = build_render_request(
        &item("https://example.com/a"),
        RenderMode::Chrome,
        Some(automation_ref("/tmp/script.json")),
    );
    assert_eq!(
        req.automation_script.map(|a| a.uri),
        Some("/tmp/script.json".to_string())
    );
}

#[test]
fn build_render_request_omits_automation_script_when_unset() {
    let req = build_render_request(&item("https://example.com/a"), RenderMode::Http, None);
    assert!(req.automation_script.is_none());
}

#[tokio::test]
async fn chrome_mode_threads_automation_script_into_render_request() {
    let providers = FakeAdapterProviders::new();
    let mut options = opts(RenderMode::Chrome, 200);
    options.automation_script = Some(automation_ref("/tmp/script.json"));
    // FakeAdapterProviders' render() echoes request.metadata but not the
    // automation_script field back onto RenderedResource, so this call
    // succeeding (rather than being rejected, as the pre-fix
    // ChromeRenderProvider stub did) is itself the regression proof at the
    // provider level; `build_render_request` unit tests above cover the exact
    // field threading.
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &options,
    )
    .await
    .unwrap();
    assert!(acquired.item.is_some());
}

// ── Regression 3: etag_conditional / 304 handling ───────────────────────────

#[test]
fn build_fetch_request_omits_conditional_header_without_prior_etag() {
    let req = build_fetch_request(&item("https://example.com/a"), None, &[]);
    assert!(req.headers.headers.is_empty());
}

#[test]
fn build_fetch_request_adds_if_none_match_with_prior_etag() {
    let req = build_fetch_request(&item("https://example.com/a"), Some("\"abc\""), &[]);
    assert_eq!(req.headers.headers.len(), 1);
    assert_eq!(req.headers.headers[0].name, "If-None-Match");
    assert_eq!(req.headers.headers[0].value, "\"abc\"");
    assert!(!req.headers.headers[0].redacted);
}

#[test]
fn build_fetch_request_preserves_custom_headers_with_prior_etag() {
    let req = build_fetch_request(
        &item("https://example.com/a"),
        Some("\"abc\""),
        &[RedactedHeader {
            name: "X-Test".to_string(),
            value: "ok".to_string(),
            redacted: false,
        }],
    );

    assert_eq!(req.headers.headers.len(), 2);
    assert_eq!(req.headers.headers[0].name, "X-Test");
    assert_eq!(req.headers.headers[1].name, "If-None-Match");
}

#[tokio::test]
async fn etag_conditional_uses_prior_overlay_not_current_discovery_etag() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/page")
                .header("If-None-Match", "\"v1\"");
            then.status(200)
                .header("content-type", "text/html; charset=utf-8")
                .header("etag", "\"v2\"")
                .body("<html><body>updated</body></html>");
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());
    let manifest_item = item_with_current_and_prior_etags(&url, "\"v2\"", "\"v1\"");

    let acquired = acquire_via_fetch(&provider, &manifest_item, true, &[])
        .await
        .unwrap()
        .expect("conditional miss should still fetch content");
    assert_eq!(acquired.metadata["web_status"], 200);
    assert_eq!(acquired.metadata["web_etag"], "\"v2\"");
    assert!(acquired.metadata.get("web_reuse_required").is_none());
}

#[tokio::test]
async fn etag_conditional_304_marks_the_item_for_reuse() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/page")
                .header("If-None-Match", "\"v1\"");
            then.status(304);
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());
    let manifest_item = item_with_current_and_prior_etags(&url, "\"v2\"", "\"v1\"");

    let result = acquire_via_fetch(&provider, &manifest_item, true, &[])
        .await
        .unwrap();
    let acquired = result.expect("304 should produce a reuse marker item");
    assert_eq!(acquired.metadata["web_status"], 304);
    assert_eq!(acquired.metadata["web_reuse_required"], true);
    assert!(matches!(acquired.content_ref, ContentRef::External { .. }));
}

#[tokio::test]
async fn etag_conditional_disabled_sends_no_conditional_header() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // Only registered mock requires the conditional header; a request that
    // omits it falls through to httpmock's default (unmatched) 404 response.
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/page")
                .header("If-None-Match", "\"v1\"");
            then.status(304);
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());
    let manifest_item = item_with_etag(&url, "\"v1\"");

    let acquired = acquire_via_fetch(&provider, &manifest_item, false, &[])
        .await
        .unwrap()
        .expect("etag_conditional=false must not skip the item");
    assert_eq!(acquired.metadata["web_status"], 404);
}

#[tokio::test]
async fn rejects_304_without_sending_a_prior_validator() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(304);
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());
    let manifest_item = item_with_etag(&url, "\"v1\"");

    let err = acquire_via_fetch(&provider, &manifest_item, false, &[])
        .await
        .expect_err("304 without a sent validator must fail");
    assert_eq!(
        err.code.to_string(),
        "web.fetch.invalid_304_without_validator"
    );
    assert!(err.message.contains("304 Not Modified"));
}

#[tokio::test]
async fn etag_conditional_200_updates_stored_etag() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200)
                .header("etag", "\"v2\"")
                .header("content-type", "text/plain")
                .body("fresh content");
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());
    let manifest_item = item_with_etag(&url, "\"v1\"");

    let acquired = acquire_via_fetch(&provider, &manifest_item, true, &[])
        .await
        .unwrap()
        .expect("200 must not be skipped");
    assert_eq!(acquired.metadata["web_etag"], "\"v2\"");
}

#[tokio::test]
async fn no_prior_etag_still_fetches_normally_when_conditional_enabled() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/page");
            then.status(200)
                .header("etag", "\"first\"")
                .header("content-type", "text/plain")
                .body("content");
        })
        .await;

    let provider = HttpFetchProvider::new(HttpFetchConfig::default());
    let url = format!("{}/page", server.base_url());

    let acquired = acquire_via_fetch(&provider, &item(&url), true, &[])
        .await
        .unwrap()
        .expect("first fetch with no prior etag must not be skipped");
    assert_eq!(acquired.metadata["web_etag"], "\"first\"");
}

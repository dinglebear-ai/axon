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

fn opts(mode: RenderMode, min_markdown_chars: usize) -> AcquireOptions {
    AcquireOptions {
        mode,
        min_markdown_chars,
        automation_script: None,
        etag_conditional: false,
    }
}

#[tokio::test]
async fn http_mode_calls_fetch_only_and_defaults_content_kind_to_html() {
    let providers = FakeAdapterProviders::new();
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &opts(RenderMode::Http, 200),
    )
    .await
    .unwrap()
    .expect("http fetch should not be skipped");

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
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &opts(RenderMode::Chrome, 200),
    )
    .await
    .unwrap()
    .expect("chrome render should not be skipped");

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
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &opts(RenderMode::AutoSwitch, 5),
    )
    .await
    .unwrap()
    .expect("auto-switch should not be skipped");

    assert_eq!(providers.calls().await, vec!["render"]);
    assert_eq!(acquired.metadata["web_fetch_method"], "auto_switch_http");
}

#[tokio::test]
async fn auto_switch_re_renders_with_chrome_when_thin() {
    let providers = FakeAdapterProviders::new();
    // The fake's fixed "fake render" body (11 chars) is thin against a high
    // threshold, so a second (Chrome) render must occur.
    let acquired = acquire_item(
        &providers,
        &providers,
        &item("https://example.com/docs/intro"),
        &opts(RenderMode::AutoSwitch, 1000),
    )
    .await
    .unwrap()
    .expect("auto-switch should not be skipped");

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
    assert!(acquired.is_some());
}

// ── Regression 3: etag_conditional / 304 handling ───────────────────────────

#[test]
fn build_fetch_request_omits_conditional_header_without_prior_etag() {
    let req = build_fetch_request(&item("https://example.com/a"), None);
    assert!(req.headers.headers.is_empty());
}

#[test]
fn build_fetch_request_adds_if_none_match_with_prior_etag() {
    let req = build_fetch_request(&item("https://example.com/a"), Some("\"abc\""));
    assert_eq!(req.headers.headers.len(), 1);
    assert_eq!(req.headers.headers[0].name, "If-None-Match");
    assert_eq!(req.headers.headers[0].value, "\"abc\"");
    assert!(!req.headers.headers[0].redacted);
}

#[tokio::test]
async fn etag_conditional_304_skips_the_item() {
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
    let manifest_item = item_with_etag(&url, "\"v1\"");

    let result = acquire_via_fetch(&provider, &manifest_item, true)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "304 Not Modified must be treated as unchanged and skipped"
    );
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

    let acquired = acquire_via_fetch(&provider, &manifest_item, false)
        .await
        .unwrap()
        .expect("etag_conditional=false must not skip the item");
    assert_eq!(acquired.metadata["web_status"], 404);
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

    let acquired = acquire_via_fetch(&provider, &manifest_item, true)
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

    let acquired = acquire_via_fetch(&provider, &item(&url), true)
        .await
        .unwrap()
        .expect("first fetch with no prior etag must not be skipped");
    assert_eq!(acquired.metadata["web_etag"], "\"first\"");
}

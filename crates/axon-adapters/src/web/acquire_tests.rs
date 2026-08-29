use axon_api::source::*;
use httpmock::prelude::*;
use std::time::Duration;

use crate::adapter::{AcquisitionProgress, AcquisitionProgressSink};
use crate::boundary::FakeAdapterProviders;
use crate::providers::http_fetch::{HttpFetchConfig, HttpFetchProvider};

use super::*;

#[test]
fn acquisition_timing_summary_exposes_tail_gaps_and_slot_occupancy() {
    let summary = summarize_acquisition_timings(
        Duration::from_millis(100),
        2,
        &[
            ItemTiming {
                elapsed: Duration::from_millis(10),
                completed_at: Duration::from_millis(10),
            },
            ItemTiming {
                elapsed: Duration::from_millis(20),
                completed_at: Duration::from_millis(20),
            },
            ItemTiming {
                elapsed: Duration::from_millis(80),
                completed_at: Duration::from_millis(80),
            },
        ],
    );

    assert_eq!(summary.wall_ms, 100);
    assert_eq!(summary.first_completion_ms, 10);
    assert_eq!(summary.item_p50_ms, 20);
    assert_eq!(summary.item_p95_ms, 80);
    assert_eq!(summary.item_max_ms, 80);
    assert_eq!(summary.max_completion_gap_ms, 60);
    assert_eq!(summary.slot_occupancy_permille, 550);
}

#[test]
fn acquisition_timing_summary_handles_empty_and_zero_capacity_batches() {
    let empty = summarize_acquisition_timings(Duration::ZERO, 0, &[]);
    assert_eq!(
        empty,
        AcquisitionTimingSummary {
            wall_ms: 0,
            first_completion_ms: 0,
            item_p50_ms: 0,
            item_p95_ms: 0,
            item_max_ms: 0,
            max_completion_gap_ms: 0,
            slot_occupancy_permille: 0,
        }
    );

    let zero_capacity = summarize_acquisition_timings(
        Duration::from_millis(10),
        0,
        &[ItemTiming {
            elapsed: Duration::from_millis(7),
            completed_at: Duration::from_millis(8),
        }],
    );
    assert_eq!(zero_capacity.first_completion_ms, 8);
    assert_eq!(zero_capacity.item_p50_ms, 7);
    assert_eq!(zero_capacity.item_p95_ms, 7);
    assert_eq!(zero_capacity.max_completion_gap_ms, 0);
    assert_eq!(zero_capacity.slot_occupancy_permille, 0);
}

#[test]
fn acquisition_timing_summary_does_not_invent_milliseconds() {
    let summary = summarize_acquisition_timings(
        Duration::from_micros(900),
        4,
        &[ItemTiming {
            elapsed: Duration::from_micros(600),
            completed_at: Duration::from_micros(700),
        }],
    );

    assert_eq!(summary.wall_ms, 0);
    assert_eq!(summary.first_completion_ms, 0);
    assert_eq!(summary.item_p95_ms, 0);
    assert_eq!(summary.slot_occupancy_permille, 0);
}

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
        headers: Vec::new(),
        render_metadata: MetadataMap::new(),
        cache_policy: CachePolicy::Bypass,
        vertical: VerticalOptions {
            enabled: false,
            auto_dispatch_skip: Vec::new(),
            user_agent: None,
            cache_ttl_secs: Default::default(),
        },
    }
}

#[derive(Default)]
struct RecordingProgress(tokio::sync::Mutex<Vec<AcquisitionProgress>>);

#[async_trait::async_trait]
impl AcquisitionProgressSink for RecordingProgress {
    async fn report(&self, progress: AcquisitionProgress) {
        self.0.lock().await.push(progress);
    }
}

fn require_item(outcome: AcquiredItem, message: &str) -> AcquiredSourceItem {
    assert!(outcome.warnings.is_empty(), "unexpected warning");
    outcome.item.expect(message)
}

#[tokio::test]
async fn concurrent_acquisition_reports_each_completed_page() {
    let providers = FakeAdapterProviders::new();
    let progress = RecordingProgress::default();
    let manifest_items = vec![
        item("https://example.com/docs/one"),
        item("https://example.com/docs/two"),
        item("https://example.com/docs/three"),
    ];

    let (items, warnings) = acquire_concurrent(
        &providers,
        &providers,
        &manifest_items,
        &opts(RenderMode::Http, 200),
        Some(&progress),
    )
    .await;

    assert!(warnings.is_empty());
    assert_eq!(items.len(), 3);
    let snapshots = progress.0.lock().await;
    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        snapshots.last(),
        Some(&AcquisitionProgress {
            items_total: 3,
            items_done: 3,
            documents_done: 3,
        })
    );
}

#[tokio::test]
async fn sequential_acquisition_reports_each_completed_page() {
    let providers = FakeAdapterProviders::new();
    let progress = RecordingProgress::default();
    let manifest_items = vec![
        item("https://example.com/docs/one"),
        item("https://example.com/docs/two"),
        item("https://example.com/docs/three"),
    ];

    let (items, warnings) = acquire_sequential(
        &providers,
        &providers,
        &manifest_items,
        &opts(RenderMode::Http, 200),
        Some(&progress),
    )
    .await;

    assert!(warnings.is_empty());
    assert_eq!(items.len(), 3);
    let snapshots = progress.0.lock().await;
    assert_eq!(snapshots.len(), 3);
    assert_eq!(snapshots[0].items_done, 1);
    assert_eq!(snapshots[1].items_done, 2);
    assert_eq!(
        snapshots.last(),
        Some(&AcquisitionProgress {
            items_total: 3,
            items_done: 3,
            documents_done: 3,
        })
    );
}

#[tokio::test]
async fn failed_pages_advance_attempts_without_inflating_documents() {
    let providers = FakeAdapterProviders::new().with_mode(crate::boundary::FakeAdapterMode::Fatal);
    let progress = RecordingProgress::default();
    let manifest_items = vec![
        item("https://example.com/docs/one"),
        item("https://example.com/docs/two"),
    ];

    let (items, warnings) = acquire_concurrent(
        &providers,
        &providers,
        &manifest_items,
        &opts(RenderMode::Http, 200),
        Some(&progress),
    )
    .await;

    assert!(items.is_empty());
    assert_eq!(warnings.len(), 2);
    let snapshots = progress.0.lock().await;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(
        snapshots.last(),
        Some(&AcquisitionProgress {
            items_total: 2,
            items_done: 2,
            documents_done: 0,
        })
    );
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
async fn auto_switch_pdf_url_bypasses_renderer_and_preserves_binary_content() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/agenda.PDF");
            then.status(200)
                .header("content-type", "application/pdf")
                .body(b"%PDF-1.7\n\0binary-payload");
        })
        .await;

    let fetch = HttpFetchProvider::new(HttpFetchConfig::default());
    let render = FakeAdapterProviders::new();
    let url = format!("{}/agenda.PDF?download=1#page=1", server.base_url());
    let acquired = require_item(
        acquire_item(
            &fetch,
            &render,
            &item(&url),
            &opts(RenderMode::AutoSwitch, 5),
        )
        .await
        .unwrap(),
        "PDF fetch should not be skipped",
    );

    assert!(render.calls().await.is_empty());
    assert_eq!(
        acquired.manifest_item.content_kind,
        Some(ContentKind::BinaryMetadata)
    );
    assert!(matches!(
        acquired.content_ref,
        ContentRef::InlineBytes { .. }
    ));
    assert_eq!(acquired.metadata["web_fetch_method"], "http_fetch");
    assert_eq!(acquired.metadata["web_render_bypass_reason"], "pdf_uri");
}

#[test]
fn pdf_uri_detection_handles_case_query_and_fragment_without_false_suffixes() {
    assert!(uri_has_pdf_path(
        "https://example.com/agenda.PDF?download=1#page=2"
    ));
    assert!(!uri_has_pdf_path("https://example.com/agenda.pdf.html"));
    assert!(!uri_has_pdf_path(
        "https://example.com/view?file=agenda.pdf"
    ));
}

#[test]
fn rendered_pdf_magic_is_rejected_before_markdown_processing() {
    let manifest_item = item("https://example.com/download");
    let err = reject_binary_rendered_payload(
        &manifest_item,
        "%PDF-1.7\n\0binary bytes incorrectly decoded as text",
    )
    .expect_err("raw PDF bytes must never enter the markdown pipeline");

    assert_eq!(err.code.to_string(), "web.render.binary_payload");
    assert!(err.message.contains("binary content as markdown"));
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
        MetadataMap::new(),
    );
    assert_eq!(
        req.automation_script.map(|a| a.uri),
        Some("/tmp/script.json".to_string())
    );
}

#[test]
fn build_render_request_omits_automation_script_when_unset() {
    let req = build_render_request(
        &item("https://example.com/a"),
        RenderMode::Http,
        None,
        MetadataMap::new(),
    );
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
    let req = build_fetch_request(&item("https://example.com/a"), None, None, &[]);
    assert!(req.headers.headers.is_empty());
}

#[test]
fn build_fetch_request_adds_if_none_match_with_prior_etag() {
    let req = build_fetch_request(&item("https://example.com/a"), Some("\"abc\""), None, &[]);
    assert_eq!(req.headers.headers.len(), 1);
    assert_eq!(req.headers.headers[0].name, "If-None-Match");
    assert_eq!(req.headers.headers[0].value, "\"abc\"");
    assert!(!req.headers.headers[0].redacted);
}

#[test]
fn build_fetch_request_adds_if_modified_since_with_prior_last_modified() {
    let req = build_fetch_request(
        &item("https://example.com/a"),
        None,
        Some("Wed, 21 Oct 2026 07:28:00 GMT"),
        &[],
    );
    assert_eq!(req.headers.headers.len(), 1);
    assert_eq!(req.headers.headers[0].name, "If-Modified-Since");
    assert_eq!(
        req.headers.headers[0].value,
        "Wed, 21 Oct 2026 07:28:00 GMT"
    );
    assert!(!req.headers.headers[0].redacted);
}

#[test]
fn build_fetch_request_preserves_custom_headers_with_prior_etag() {
    let req = build_fetch_request(
        &item("https://example.com/a"),
        Some("\"abc\""),
        None,
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

    let acquired = acquire_via_fetch(&provider, &manifest_item, CachePolicy::Revalidate, &[])
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

    let result = acquire_via_fetch(&provider, &manifest_item, CachePolicy::Revalidate, &[])
        .await
        .unwrap();
    let acquired = result.expect("304 should produce a reuse marker item");
    assert_eq!(acquired.metadata["web_status"], 304);
    assert_eq!(acquired.metadata["web_reuse_required"], true);
    assert!(matches!(acquired.content_ref, ContentRef::External { .. }));
}

#[tokio::test]
async fn conditional_304_advances_progress_with_a_reusable_document() {
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
    let render = FakeAdapterProviders::new();
    let progress = RecordingProgress::default();
    let url = format!("{}/page", server.base_url());
    let manifest_items = vec![item_with_current_and_prior_etags(&url, "\"v2\"", "\"v1\"")];
    let mut options = opts(RenderMode::Http, 200);
    options.cache_policy = CachePolicy::Revalidate;

    let (items, warnings) = acquire_concurrent(
        &provider,
        &render,
        &manifest_items,
        &options,
        Some(&progress),
    )
    .await;

    assert!(warnings.is_empty());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].metadata["web_reuse_required"], true);
    assert_eq!(
        progress.0.lock().await.last(),
        Some(&AcquisitionProgress {
            items_total: 1,
            items_done: 1,
            documents_done: 1,
        })
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

    let acquired = acquire_via_fetch(&provider, &manifest_item, CachePolicy::Bypass, &[])
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

    let err = acquire_via_fetch(&provider, &manifest_item, CachePolicy::Bypass, &[])
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

    let acquired = acquire_via_fetch(&provider, &manifest_item, CachePolicy::Revalidate, &[])
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

    let acquired = acquire_via_fetch(&provider, &item(&url), CachePolicy::Revalidate, &[])
        .await
        .unwrap()
        .expect("first fetch with no prior etag must not be skipped");
    assert_eq!(acquired.metadata["web_etag"], "\"first\"");
}

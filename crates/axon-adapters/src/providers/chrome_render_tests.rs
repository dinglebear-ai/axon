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

#[tokio::test]
async fn ninth_render_waits_for_a_page_slot() {
    let provider = provider();
    let mut permits = Vec::new();
    for _ in 0..REMOTE_CHROME_MAX_CONCURRENT_PAGES {
        permits.push(provider.acquire_page_slot().await.expect("page slot"));
    }

    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            provider.acquire_page_slot()
        )
        .await
        .is_err(),
        "the ninth render must wait while all page slots are occupied"
    );

    permits.pop();
    let _released_permit = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        provider.acquire_page_slot(),
    )
    .await
    .expect("released capacity should wake waiter")
    .expect("page slot should remain open");
}

#[tokio::test]
async fn http_render_does_not_consume_a_chrome_page_slot() {
    let provider = provider();
    let mut permits = Vec::new();
    for _ in 0..REMOTE_CHROME_MAX_CONCURRENT_PAGES {
        permits.push(provider.acquire_page_slot().await.expect("page slot"));
    }

    let permit = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        provider.acquire_page_slot_for(
            CoreRenderMode::Http,
            tokio::time::Instant::now() + std::time::Duration::from_millis(100),
        ),
    )
    .await
    .expect("HTTP render must not wait for Chrome capacity")
    .expect("capacity must remain open");
    assert!(permit.is_none());
}

#[tokio::test]
async fn chrome_page_capacity_wait_is_bounded_by_deadline() {
    let provider = ChromeRenderProvider::new(ChromeRenderConfig {
        max_concurrent_pages: Some(1),
        ..ChromeRenderConfig::default()
    });
    let _held = provider.acquire_page_slot().await.expect("page slot");

    let error = provider
        .acquire_page_slot_for(
            CoreRenderMode::Chrome,
            tokio::time::Instant::now() + std::time::Duration::from_millis(10),
        )
        .await
        .expect_err("capacity admission must time out");

    assert_eq!(error.code.to_string(), "render.timeout");
}

#[tokio::test]
async fn invalid_uri_is_rejected_while_chrome_capacity_is_exhausted() {
    let provider = ChromeRenderProvider::new(ChromeRenderConfig {
        max_concurrent_pages: Some(1),
        ..ChromeRenderConfig::default()
    });
    let _held = provider.acquire_page_slot().await.expect("page slot");
    let mut request = request("http://127.0.0.1/admin".to_string(), RenderMode::Chrome);
    request.timeout_ms = Some(10);

    let error = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        provider.render(request),
    )
    .await
    .expect("SSRF rejection must not wait for Chrome capacity")
    .expect_err("private URL must be rejected");

    assert_eq!(error.code.to_string(), "render.invalid_uri");
}

#[tokio::test]
async fn render_rejects_private_and_local_schemes_before_browser_bootstrap() {
    for mode in [RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch] {
        for uri in [
            "http://127.0.0.1/admin",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/private",
            "http://[fd00::1]/private",
            "file:///etc/passwd",
        ] {
            let err = provider()
                .render(request(uri.to_string(), mode))
                .await
                .expect_err("blocked render target must fail before browser work");
            assert_eq!(err.code.to_string(), "render.invalid_uri", "target: {uri}");
        }
    }
}

#[test]
fn map_render_mode_round_trips_all_variants() {
    for mode in [RenderMode::Http, RenderMode::Chrome, RenderMode::AutoSwitch] {
        assert_eq!(map_core_render_mode(map_render_mode(mode)), mode);
    }
}

#[test]
fn render_request_metadata_configures_advertised_web_options() {
    let mut request = request("https://example.com".to_string(), RenderMode::Chrome);
    request
        .metadata
        .insert("normalize".to_string(), serde_json::json!(true));
    request
        .metadata
        .insert("block_assets".to_string(), serde_json::json!(true));
    request.metadata.insert(
        "chrome_wait_for_selector".to_string(),
        serde_json::json!("#ready"),
    );
    request
        .metadata
        .insert("root_selector".to_string(), serde_json::json!("main"));
    request
        .metadata
        .insert("exclude_selector".to_string(), serde_json::json!("aside"));
    request
        .metadata
        .insert("chrome_screenshot".to_string(), serde_json::json!(true));
    request.metadata.insert(
        "chrome_network_idle_timeout_secs".to_string(),
        serde_json::json!(1),
    );
    request
        .metadata
        .insert("format".to_string(), serde_json::json!("rawHtml"));
    request.metadata.insert(
        "output_dir".to_string(),
        serde_json::json!("/tmp/axon-output"),
    );

    let cfg = provider().build_config(&request);

    assert!(cfg.normalize);
    assert!(cfg.block_assets);
    assert_eq!(cfg.chrome_wait_for_selector.as_deref(), Some("#ready"));
    assert_eq!(cfg.root_selector.as_deref(), Some("main"));
    assert_eq!(cfg.exclude_selector.as_deref(), Some("aside"));
    assert!(cfg.chrome_screenshot);
    assert_eq!(cfg.chrome_network_idle_timeout_secs, 1);
    assert_eq!(cfg.format, ScrapeFormat::RawHtml);
    assert_eq!(cfg.output_dir, PathBuf::from("/tmp/axon-output"));
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

#[tokio::test]
async fn render_deadline_bounds_a_provider_future_that_never_resolves() {
    let error = await_isolated_render_outcome(
        std::time::Duration::from_millis(10),
        std::future::pending::<std::result::Result<(), String>>(),
    )
    .await
    .expect_err("a hung render must be bounded by the provider deadline");

    assert!(error.contains("timed out after 10ms"));
}

#[tokio::test]
async fn timed_out_blocking_render_is_reaped_after_it_yields() {
    struct Active(Arc<AtomicUsize>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    let active = Arc::new(AtomicUsize::new(1));
    let probe = Active(Arc::clone(&active));
    let error = await_isolated_render_outcome(std::time::Duration::from_millis(10), async move {
        let _probe = probe;
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::future::pending::<std::result::Result<(), String>>().await
    })
    .await
    .expect_err("blocking render must time out");
    assert!(error.contains("timed out after 10ms"));

    tokio::time::timeout(std::time::Duration::from_millis(500), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("timed-out render must be reaped after its blocking poll yields");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn render_deadlines_remain_responsive_when_all_runtime_workers_block() {
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let started = std::time::Instant::now();
    let calls = (0..2).map(|_| {
        let barrier = Arc::clone(&barrier);
        tokio::spawn(await_isolated_render_outcome(
            std::time::Duration::from_millis(10),
            async move {
                barrier.wait();
                std::thread::sleep(std::time::Duration::from_millis(200));
                std::future::pending::<std::result::Result<(), String>>().await
            },
        ))
    });
    let calls = calls.collect::<Vec<_>>();
    barrier.wait();
    let mut outcomes = Vec::new();
    for call in calls {
        outcomes.push(call.await);
    }

    assert!(outcomes.into_iter().all(|outcome| {
        outcome
            .expect("deadline task must join")
            .expect_err("blocking render must time out")
            .contains("timed out after 10ms")
    }));
    assert!(
        started.elapsed() < std::time::Duration::from_millis(100),
        "deadline controller was blocked for {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn render_deadline_arithmetic_saturates_for_untrusted_metadata() {
    let mut request = request("http://127.0.0.1:9".to_string(), RenderMode::Http);
    request.metadata.insert(
        "chrome_network_idle_timeout_secs".to_string(),
        serde_json::json!(u64::MAX),
    );
    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        provider().render(request),
    )
    .await;
    assert!(
        outcome.is_ok(),
        "deadline calculation must not overflow or panic"
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
fn classify_render_error_recognizes_retryable_server_errors() {
    assert_eq!(
        classify_render_error("scrape failed: HTTP 503 for https://x/"),
        RenderFailureClass::Transient
    );
    assert_eq!(
        classify_render_error("scrape failed: HTTP 526 for https://x/"),
        RenderFailureClass::Transient
    );
}

#[test]
fn classify_render_error_defaults_unmatched_errors_to_fatal() {
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
        created_at: Timestamp::from(Utc::now()),
    }
}

#[test]
fn automation_script_path_strips_file_scheme() {
    assert_eq!(
        automation_script_path("file:///tmp/script.json"),
        PathBuf::from("/tmp/script.json")
    );
    assert_eq!(
        automation_script_path("/tmp/script.json"),
        PathBuf::from("/tmp/script.json")
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
    let _loopback = axon_core::http::LoopbackGuard::allow();
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
    let _loopback = axon_core::http::LoopbackGuard::allow();
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
async fn render_server_error_is_retryable_and_marks_provider_degraded() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/broken");
            then.status(503);
        })
        .await;

    let provider = provider();
    let url = format!("{}/broken", server.base_url());
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let err = provider
        .render(request(url, RenderMode::Http))
        .await
        .expect_err("5xx must surface as an error");
    assert_eq!(err.code.to_string(), "render.transient");
    assert!(err.retryable);

    let capability = provider.capabilities().await.expect("capabilities");
    assert_eq!(capability.health, HealthStatus::Degraded);
}

#[tokio::test]
async fn render_rate_limited_cools_the_provider_with_cooldown_until() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/rate-limited");
            then.status(429);
        })
        .await;

    let provider = provider();
    let url = format!("{}/rate-limited", server.base_url());
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let err = provider
        .render(request(url, RenderMode::Http))
        .await
        .expect_err("429 must surface as an error");
    assert_eq!(err.code.to_string(), "render.rate_limited");
    assert!(err.retryable);

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
        max_concurrent_pages: None,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 18)]
#[ignore = "requires a live Chrome/CDP endpoint, not available in CI"]
async fn concurrent_chrome_renders_all_reach_a_terminal_outcome() {
    let provider = ChromeRenderProvider::new(ChromeRenderConfig {
        max_concurrent_pages: Some(8),
        chrome_remote_url: std::env::var("AXON_CHROME_REMOTE_URL").ok(),
        default_timeout_ms: Some(10_000),
    });
    let urls = [
        "https://nextjs.org/blog/composable-caching",
        "https://nextjs.org/blog/CVE-2025-66478",
        "https://nextjs.org/blog/august-2026-security-release",
        "https://nextjs.org/blog",
        "https://nextjs.org/blog/agentic-future",
        "https://nextjs.org/.well-known/ai-catalog.json",
        "https://nextjs.org/blog/building-app-like-experiences-with-nextjs-16-3",
        "https://nextjs.org/blog/building-apis-with-nextjs",
    ];
    let started = std::time::Instant::now();
    let outcomes = futures_util::future::join_all((0..32).map(|index| {
        let url = urls[index % urls.len()];
        let provider = provider.clone();
        async move {
            provider
                .render(request(url.to_string(), RenderMode::Chrome))
                .await
        }
    }))
    .await;

    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "concurrent renders exceeded their provider deadlines: {:?}",
        started.elapsed()
    );
    assert_eq!(outcomes.len(), 32);
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
        max_concurrent_pages: None,
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

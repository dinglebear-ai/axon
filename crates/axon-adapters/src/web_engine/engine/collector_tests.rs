use super::super::adaptive::AdaptiveCrawlControl;
use super::super::url_utils::MapScope;
use super::*;
use axon_core::config::Config;
use httpmock::prelude::*;
use std::collections::HashMap;

fn test_collector_config(scope: Option<MapScope>) -> CollectorConfig {
    CollectorConfig {
        markdown_dir: std::env::temp_dir(),
        manifest_path: std::env::temp_dir().join("collector-manifest.jsonl"),
        min_chars: 10,
        drop_thin: false,
        exclude_path_prefix: Vec::new(),
        include_subdomains: false,
        start_host: None,
        scope,
        progress_tx: None,
        previous_manifest: Arc::new(HashMap::new()),
        selector_config: None,
        chrome_ws_url: None,
        chrome_timeout_secs: 1,
        output_dir: std::env::temp_dir(),
        ladder_thresholds: axon_core::content::LadderThresholds {
            strategy1: 30,
            strategy2: 200,
            body_multiplier: 2.0,
        },
        antibot_max_scan_bytes: 150_000,
        structured_max_bytes: 65_536,
        max_depth: 5,
        retry_backoff_ms: 250,
        adaptive: None,
        max_tracked_discovered_urls: crate::web_engine::engine::MAX_TRACKED_DISCOVERED_URLS,
    }
}

#[test]
fn canonicalize_and_track_page_rejects_same_host_root_outside_project_scope() {
    let col = test_collector_config(Some(MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    }));
    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen = HashSet::new();

    let url = canonicalize_and_track_page(
        "https://example.github.io/",
        &col,
        &mut summary,
        &mut urls,
        &mut seen,
    );

    assert!(url.is_none());
    assert_eq!(summary.pages_seen, 0);
    assert!(urls.is_empty());
}

#[test]
fn canonicalize_and_track_page_accepts_in_scope_project_page() {
    let col = test_collector_config(Some(MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    }));
    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen = HashSet::new();

    let url = canonicalize_and_track_page(
        "https://example.github.io/project/docs/",
        &col,
        &mut summary,
        &mut urls,
        &mut seen,
    );

    assert_eq!(
        url.as_deref(),
        Some("https://example.github.io/project/docs")
    );
    assert_eq!(summary.pages_seen, 1);
    assert!(urls.contains("https://example.github.io/project/docs"));
}

#[test]
fn canonicalize_discovered_link_resolves_relative_links_against_page_url() {
    let col = test_collector_config(None);

    assert_eq!(
        canonicalize_discovered_link(
            "/docs/en/api/messages/",
            "https://platform.claude.com/docs/en/home",
            &col,
        )
        .as_deref(),
        Some("https://platform.claude.com/docs/en/api/messages")
    );
}

#[test]
fn canonicalize_discovered_link_rejects_cross_host_links() {
    let col = test_collector_config(None);

    assert_eq!(
        canonicalize_discovered_link(
            "https://example.com/docs/en/api/messages/",
            "https://platform.claude.com/docs/en/home",
            &col,
        ),
        None
    );
}

#[test]
fn canonicalize_discovered_link_rejects_media_assets_and_junk() {
    let col = test_collector_config(None);

    assert_eq!(
        canonicalize_discovered_link(
            "/assets/logo.png",
            "https://platform.claude.com/docs/en/home",
            &col,
        ),
        None
    );
    assert_eq!(
        canonicalize_discovered_link(
            "/$%7BshareBaseUrl%7D/s/$%7BshareId%7D",
            "https://platform.claude.com/docs/en/home",
            &col,
        ),
        None
    );
}

#[test]
fn canonicalize_discovered_link_counts_subdomain_when_enabled() {
    let mut col = test_collector_config(None);
    col.include_subdomains = true;
    col.start_host = Some("example.com".to_string());

    assert_eq!(
        canonicalize_discovered_link(
            "https://docs.example.com/guide/",
            "https://example.com/docs/en/home",
            &col,
        )
        .as_deref(),
        Some("https://docs.example.com/guide")
    );
}

#[tokio::test]
async fn process_received_page_records_adaptive_status_failures() {
    let server = MockServer::start();
    for (path, status) in [("/ok", 200), ("/limited", 429), ("/unavailable", 503)] {
        server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(status)
                .header("content-type", "text/html")
                .body("<html><body>status page</body></html>");
        });
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_file = tokio::fs::File::create(temp.path().join("manifest.jsonl"))
        .await
        .expect("manifest file");
    let mut manifest = tokio::io::BufWriter::new(manifest_file);
    let cfg = Config {
        crawl_concurrency_limit: Some(8),
        adaptive_concurrency: axon_core::config::AdaptiveConcurrencyConfig {
            enabled: true,
            min: 1,
            max: Some(8),
        },
        ..Config::default()
    };
    let adaptive = AdaptiveCrawlControl::from_config(&cfg).expect("adaptive control");
    let mut col = test_collector_config(None);
    col.markdown_dir = temp.path().to_path_buf();
    col.output_dir = temp.path().to_path_buf();
    col.manifest_path = temp.path().join("manifest.jsonl");
    col.min_chars = 1;
    col.adaptive = Some(adaptive.clone());

    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen_canonical = HashSet::new();
    let mut chrome_tasks = JoinSet::new();
    let chrome_semaphore = Arc::new(Semaphore::new(THIN_REFETCH_CONCURRENCY));
    let mut last_progress = std::time::Instant::now();
    let crawl_started = std::time::Instant::now();
    let mut discovered = HashSet::new();
    let client = spider::reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build();

    for path in ["/ok", "/limited", "/unavailable"] {
        let page =
            spider::page::Page::new_page(&format!("{}{path}", server.base_url()), &client).await;
        process_received_page(
            page,
            &col,
            &mut summary,
            &mut urls,
            &mut seen_canonical,
            &mut manifest,
            &mut chrome_tasks,
            chrome_semaphore.clone(),
            &mut last_progress,
            crawl_started,
            &mut discovered,
        )
        .await
        .expect("page should process");
    }

    manifest.flush().await.expect("flush manifest");

    assert!(
        summary
            .rate_limited
            .iter()
            .any(|host| host.host == "127.0.0.1"),
        "429 status should still register the rate-limited host: {summary:?}"
    );
    assert_eq!(
        summary.error_pages, 2,
        "429 and 503 are skipped as error pages"
    );
    let snapshot = adaptive.snapshot();
    assert!(
        snapshot.failures >= 2,
        "429 and 503 should both count as adaptive failures: {snapshot:?}"
    );
    assert!(
        snapshot.current_target < 8,
        "adaptive target should shrink after failures: {snapshot:?}"
    );
}

#[tokio::test]
async fn process_received_page_does_not_count_challenge_200_as_adaptive_success() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/blocked");
        then.status(200).header("content-type", "text/html").body(
            "<html><body><h1>Just a moment</h1><div class=\"cf-spinner\"></div></body></html>",
        );
    });

    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_file = tokio::fs::File::create(temp.path().join("manifest.jsonl"))
        .await
        .expect("manifest file");
    let mut manifest = tokio::io::BufWriter::new(manifest_file);
    let cfg = Config {
        crawl_concurrency_limit: Some(8),
        adaptive_concurrency: axon_core::config::AdaptiveConcurrencyConfig {
            enabled: true,
            min: 1,
            max: Some(8),
        },
        ..Config::default()
    };
    let adaptive = AdaptiveCrawlControl::from_config(&cfg).expect("adaptive control");
    let mut col = test_collector_config(None);
    col.markdown_dir = temp.path().to_path_buf();
    col.output_dir = temp.path().to_path_buf();
    col.manifest_path = temp.path().join("manifest.jsonl");
    col.min_chars = 1;
    col.adaptive = Some(adaptive.clone());

    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen_canonical = HashSet::new();
    let mut chrome_tasks = JoinSet::new();
    let chrome_semaphore = Arc::new(Semaphore::new(THIN_REFETCH_CONCURRENCY));
    let mut last_progress = std::time::Instant::now();
    let crawl_started = std::time::Instant::now();
    let mut discovered = HashSet::new();
    let client = spider::reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build();
    let page =
        spider::page::Page::new_page(&format!("{}/blocked", server.base_url()), &client).await;

    process_received_page(
        page,
        &col,
        &mut summary,
        &mut urls,
        &mut seen_canonical,
        &mut manifest,
        &mut chrome_tasks,
        chrome_semaphore,
        &mut last_progress,
        crawl_started,
        &mut discovered,
    )
    .await
    .expect("page should process");

    let snapshot = adaptive.snapshot();
    assert_eq!(
        snapshot.successes, 0,
        "soft-block 200 pages must not be adaptive successes"
    );
    assert_eq!(
        snapshot.failures, 1,
        "challenge pages should apply adaptive pressure"
    );
}

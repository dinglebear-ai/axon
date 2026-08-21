use super::*;
use async_trait::async_trait;
use axon_core::config::Config;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn discovery_config_has_no_disk_output_contract() {
    let plan = crate::web_tests::web_plan("https://example.com/docs", SourceScope::Docs);

    let cfg = build_discovery_config(&plan);

    assert!(cfg.output_dir.as_os_str().is_empty());
    assert!(!cfg.cache);
}

#[test]
fn map_strategy_has_no_crawl_or_disk_handoff() {
    let strategy = include_str!("../web_engine/engine/map/strategy.rs");

    for forbidden in [
        "configure_website",
        ".crawl()",
        ".crawl_raw()",
        "output_dir",
        "manifest.jsonl",
        concat!("map_with_", "sitemap"),
    ] {
        assert!(
            !strategy.contains(forbidden),
            "bounded map strategy must not contain {forbidden}"
        );
    }
}

#[test]
fn manifest_limit_applies_to_map_items_after_sort_and_dedup() {
    let plan = crate::web_tests::web_plan("https://example.com/docs", SourceScope::Map);
    let item = |url: &str| {
        let web = WebUrlParts::parse(url).unwrap();
        web_manifest_item(&plan, &web, None, None, None)
    };

    let items = finalize_items(
        vec![
            item("https://example.com/docs/z"),
            item("https://example.com/docs/a"),
            item("https://example.com/docs/a"),
            item("https://example.com/docs/m"),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].canonical_uri.as_str(),
        "https://example.com/docs/a"
    );
    assert_eq!(
        items[1].canonical_uri.as_str(),
        "https://example.com/docs/m"
    );
}

#[test]
fn web_url_parts_preserve_benign_query_metadata_but_strip_credentials() {
    let web = WebUrlParts::parse(
        "https://example.com/docs?q=rust&page=2&pageToken=next-42&tokenCount=4&X-Amz-Date=20260819T120000Z&X-Amz-Expires=300&X-Amz-Signature=secret&accessToken=secret&utm_source=test",
    )
    .unwrap();
    assert_eq!(
        web.normalized_url,
        "https://example.com/docs?X-Amz-Date=20260819T120000Z&X-Amz-Expires=300&page=2&pageToken=next-42&q=rust&tokenCount=4"
    );
    assert!(!web.normalized_url.contains("X-Amz-Signature"));
    assert!(!web.normalized_url.contains("accessToken"));
    assert!(!web.normalized_url.contains("utm_source"));
}

#[test]
fn finalization_does_not_guess_that_independently_discovered_markdown_is_an_alternate() {
    let plan = crate::web_tests::web_plan("https://example.com/docs", SourceScope::Docs);
    let item = |url: &str| {
        let web = WebUrlParts::parse(url).unwrap();
        web_manifest_item(&plan, &web, None, None, None)
    };

    let items = finalize_items(
        vec![
            item("https://example.com/docs/authorization"),
            item("https://example.com/docs/authorization.md"),
            item("https://example.com/docs/other"),
        ],
        10,
    );

    assert_eq!(items.len(), 3);
    assert!(
        items
            .iter()
            .any(|item| item.canonical_uri.ends_with("/authorization"))
    );
    assert!(
        items
            .iter()
            .any(|item| item.canonical_uri.ends_with("/authorization.md"))
    );
}

#[tokio::test]
async fn map_discovery_uses_the_injected_fetch_provider() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = httpmock::MockServer::start();
    let providers = crate::boundary::FakeAdapterProviders::new()
        .with_fetch_text("<a href=\"/docs/provider-only\">provider result</a>");
    let providers_for_adapter = Arc::new(providers.clone());
    let adapter =
        crate::web::WebSourceAdapter::new(providers_for_adapter.clone(), providers_for_adapter);
    let mut plan = crate::web_tests::web_plan(&server.url("/docs"), SourceScope::Map);
    plan.route
        .validated_options
        .values
        .insert("discover_sitemaps".to_string(), serde_json::json!(false));
    plan.route
        .validated_options
        .values
        .insert("discover_llms_txt".to_string(), serde_json::json!(false));

    let manifest = crate::SourceAdapter::discover(&adapter, &plan)
        .await
        .unwrap();

    assert_eq!(manifest.scope, SourceScope::Map);
    assert!(
        manifest
            .items
            .iter()
            .any(|item| item.canonical_uri.ends_with("/provider-only")),
        "Map discovery must consume content returned by the injected FetchProvider"
    );
    assert!(
        providers.calls().await.contains(&"fetch"),
        "Map discovery must use the adapter's FetchProvider rather than a private HTTP client"
    );
    assert!(
        !providers.calls().await.contains(&"render"),
        "fast provider discovery must not pay for a browser render"
    );
}

#[derive(Clone)]
struct BlockedFetch;

#[async_trait]
impl FetchProvider for BlockedFetch {
    async fn fetch(&self, _request: FetchRequest) -> crate::boundary::Result<FetchedResource> {
        Err(ApiError::new(
            "fetch.blocked",
            ErrorStage::Fetching,
            "HTTP 403",
        ))
    }

    async fn capabilities(&self) -> crate::boundary::Result<ProviderCapability> {
        unreachable!("capabilities are not needed by map discovery")
    }
}

#[derive(Clone)]
struct EmptySitemapFetch;

#[async_trait]
impl FetchProvider for EmptySitemapFetch {
    async fn fetch(&self, request: FetchRequest) -> crate::boundary::Result<FetchedResource> {
        let is_seed = request.uri.ends_with('/');
        let is_empty_sitemap = request.uri.ends_with("/sitemap.xml");
        let (status, text) = if is_empty_sitemap {
            (
                200,
                "<?xml version=\"1.0\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"></urlset>",
            )
        } else if is_seed {
            (200, "<html></html>")
        } else {
            (404, "")
        };
        Ok(FetchedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            status,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: Timestamp("2026-08-02T00:00:00Z".to_string()),
            etag: None,
            redirect_chain: Vec::new(),
            bytes: Some(text.len() as u64),
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> crate::boundary::Result<ProviderCapability> {
        unreachable!("capabilities are not needed by map discovery")
    }
}

#[derive(Clone)]
struct RootNavigationRender {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl RenderProvider for RootNavigationRender {
    async fn render(&self, request: RenderRequest) -> crate::boundary::Result<RenderedResource> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.timeout_ms, Some(8_000));
        assert_eq!(request.metadata["block_assets"], true);
        assert_eq!(request.metadata["chrome_network_idle_timeout_secs"], 1);
        Ok(RenderedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            markdown: String::new(),
            html: Some(
                r#"<nav><a href="/news">News</a><a href="/calendar">Calendar</a></nav>"#
                    .to_string(),
            ),
            text: None,
            render_mode: RenderMode::Chrome,
            captured_at: Timestamp("2026-08-02T00:00:00Z".to_string()),
            artifacts: Vec::new(),
            console: Vec::new(),
            network: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> crate::boundary::Result<ProviderCapability> {
        unreachable!("capabilities are not needed by map discovery")
    }
}

#[derive(Clone)]
struct SlowRender {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl RenderProvider for SlowRender {
    async fn render(&self, _request: RenderRequest) -> crate::boundary::Result<RenderedResource> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        unreachable!("map deadline must cancel a stalled renderer")
    }

    async fn capabilities(&self) -> crate::boundary::Result<ProviderCapability> {
        unreachable!("capabilities are not needed by map discovery")
    }
}

#[tokio::test]
async fn map_uses_one_root_browser_render_only_after_fast_discovery_is_empty() {
    let calls = Arc::new(AtomicUsize::new(0));
    let adapter = crate::web::WebSourceAdapter::new(
        Arc::new(BlockedFetch),
        Arc::new(RootNavigationRender {
            calls: Arc::clone(&calls),
        }),
    );
    let mut plan = crate::web_tests::web_plan("https://example.com/", SourceScope::Map);
    plan.route
        .validated_options
        .values
        .insert("discover_sitemaps".to_string(), serde_json::json!(false));
    plan.route
        .validated_options
        .values
        .insert("discover_llms_txt".to_string(), serde_json::json!(false));

    let manifest = crate::SourceAdapter::discover(&adapter, &plan)
        .await
        .expect("rendered root navigation should recover map discovery");

    assert_eq!(calls.load(Ordering::SeqCst), 1, "render root at most once");
    let urls = manifest
        .items
        .iter()
        .map(|item| item.canonical_uri.as_str())
        .collect::<Vec<_>>();
    assert!(urls.iter().any(|url| url.ends_with("/news")), "{urls:?}");
    assert!(
        urls.iter().any(|url| url.ends_with("/calendar")),
        "{urls:?}"
    );
}

#[tokio::test]
async fn map_root_browser_render_has_an_outer_deadline() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = httpmock::MockServer::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let cfg = Config {
        discover_sitemaps: false,
        discover_llms_txt: false,
        render_mode: axon_core::config::RenderMode::Chrome,
        request_timeout_ms: Some(20),
        ..Config::default()
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        crate::web_engine::engine::discover_site_urls(
            &cfg,
            &server.url("/"),
            Arc::new(BlockedFetch),
            Arc::new(SlowRender {
                calls: Arc::clone(&calls),
            }),
        ),
    )
    .await
    .expect("map discovery must enforce an outer render deadline")
    .expect("a render timeout is represented in the map result");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(result.outcome.as_str(), "failed");
    assert!(
        result
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"),
        "{:?}",
        result.warning
    );
}

#[tokio::test]
async fn sitemap_only_fetch_failure_is_not_reported_as_empty_success() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = httpmock::MockServer::start();
    let cfg = Config {
        sitemap_only: true,
        discover_sitemaps: true,
        discover_llms_txt: false,
        render_mode: axon_core::config::RenderMode::Http,
        ..Config::default()
    };

    let result = crate::web_engine::engine::discover_site_urls(
        &cfg,
        &server.url("/"),
        Arc::new(BlockedFetch),
        Arc::new(RootNavigationRender {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await
    .expect("sitemap failure is represented in the map result");

    assert_eq!(result.outcome.as_str(), "failed");
    assert!(result.warning.is_some());
}

#[tokio::test]
async fn sitemap_only_successful_empty_sitemap_remains_empty_without_warning() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = httpmock::MockServer::start();
    let cfg = Config {
        sitemap_only: true,
        discover_sitemaps: true,
        discover_llms_txt: false,
        render_mode: axon_core::config::RenderMode::Http,
        ..Config::default()
    };

    let result = crate::web_engine::engine::discover_site_urls(
        &cfg,
        &server.url("/"),
        Arc::new(EmptySitemapFetch),
        Arc::new(RootNavigationRender {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await
    .expect("a parsed empty sitemap is a valid empty map");

    assert_eq!(result.outcome.as_str(), "empty");
    assert!(result.warning.is_none(), "{:?}", result.warning);
}

use super::*;
use axon_core::config::Config;
use axon_core::http::LoopbackGuard;
use httpmock::MockServer;

fn searx_json(results: &[(&str, &str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "results": results
            .iter()
            .map(|(url, title, content)| {
                serde_json::json!({ "url": url, "title": title, "content": content })
            })
            .collect::<Vec<_>>()
    })
}

/// `run_search` (used by `search`/`search_batch`) must respect `offset`:
/// windowing (skip `offset`, take `limit`) is applied by the delegated
/// provider, not re-applied by the caller.
#[tokio::test]
async fn run_search_windows_by_offset_against_searxng() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search")
                .query_param("pageno", "1");
            then.status(200).json_body(searx_json(&[
                ("https://a.test/1", "1", "c1"),
                ("https://a.test/2", "2", "c2"),
                ("https://a.test/3", "3", "c3"),
            ]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();

    let items = run_search(&cfg, "q", 1, 2, None, "search")
        .await
        .expect("search should succeed");
    assert_eq!(items.len(), 1, "offset=2, limit=1 should return exactly 1");
    assert_eq!(items[0].url, "https://a.test/3");
}

/// `time_range` must be forwarded to SearXNG as a `time_range` query param.
#[tokio::test]
async fn run_search_forwards_time_range_to_searxng() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search")
                .query_param("time_range", "week");
            then.status(200)
                .json_body(searx_json(&[("https://a.test/1", "1", "c1")]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();

    let items = run_search(&cfg, "q", 5, 0, Some(TimeRange::Week), "search")
        .await
        .expect("search should succeed");
    assert_eq!(items.len(), 1);
}

/// Cross-page dedup, an existing `SearxngSearchProvider` guarantee, survives
/// delegation: a duplicate URL on a later page must not double-count toward
/// `limit`.
#[tokio::test]
async fn run_search_dedupes_across_pages_against_searxng() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search")
                .query_param("pageno", "1");
            then.status(200).json_body(searx_json(&[
                ("https://a.test/1", "1", "c1"),
                ("https://a.test/2", "2", "c2"),
            ]));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/search")
                .query_param("pageno", "2");
            then.status(200).json_body(searx_json(&[
                ("https://a.test/1", "dup", "dup"),
                ("https://a.test/3", "3", "c3"),
            ]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();

    // limit=3 forces a walk into page 2 because page 1 yields only 2 unique hits.
    let items = run_search(&cfg, "q", 3, 0, None, "search")
        .await
        .expect("search should succeed");
    assert_eq!(items.len(), 3);
    assert_eq!(items[2].url, "https://a.test/3");
}

/// SearXNG is preferred over Tavily whenever `cfg.searxng_url` is set, even
/// if a Tavily key is also configured. This preserves the old client fallback
/// order.
#[tokio::test]
async fn searxng_is_preferred_over_tavily_when_both_configured() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(200)
                .json_body(searx_json(&[("https://a.test/1", "1", "c1")]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.searxng_url = server.base_url();
    cfg.tavily_api_key = "tvly-unused-key".to_string();

    let items = run_search(&cfg, "q", 1, 0, None, "search")
        .await
        .expect("search should hit searxng, not tavily");
    assert_eq!(items[0].url, "https://a.test/1");
}

/// A missing Tavily key must fail fast with the op-specific message, for
/// both `run_search` (op="search") and `run_search_for_research`
/// (op="research"). No network call or retry is attempted.
#[tokio::test]
async fn missing_tavily_key_surfaces_op_specific_error() {
    let cfg = Config {
        tavily_api_key: String::new(),
        searxng_url: String::new(),
        ..Config::test_default()
    };
    assert!(cfg.searxng_url.is_empty());
    assert!(cfg.tavily_api_key.is_empty());

    let err = run_search(&cfg, "q", 1, 0, None, "search")
        .await
        .expect_err("empty tavily key must error");
    assert!(err.to_string().contains("TAVILY_API_KEY"));
    assert!(err.to_string().contains("search"));

    let err = run_search_for_research(&cfg, "q", 1, 0, None, "research")
        .await
        .expect_err("empty tavily key must error");
    assert!(err.to_string().contains("TAVILY_API_KEY"));
    assert!(err.to_string().contains("research"));
}

//! Contract tests for `map_payload` output shape and dedup semantics.
//!
//! These lock the expected behavior AFTER the migration that moves sitemap
//! URL merge/dedup from CLI `map.rs` into the crawl engine. The tests
//! verify three contracts:
//!
//! 1. URLs in the output are unique WITHOUT CLI-side dedup (engine handles it).
//! 2. `sitemap_urls` count is reported consistently with `mapped_urls` and `pages_seen`.
//! 3. AutoSwitch only falls back to Chrome when `pages_seen == 0`.
//!
//! Tests use `httpmock` to provide sitemap XML and a stub robots.txt, then
//! call `map_payload` directly. Spider's crawl over a mock server yields
//! minimal results — the sitemap discovery path is what exercises the
//! dedup and counting logic.

use super::map_payload;
use crate::core::config::{Config, RenderMode};
use crate::core::http::set_allow_loopback;
use httpmock::prelude::*;
use serial_test::serial;

/// RAII guard: sets the global loopback bypass to `true` on creation
/// and restores `false` on drop.
struct LoopbackGuard;

impl LoopbackGuard {
    fn new() -> Self {
        set_allow_loopback(true);
        Self
    }
}

impl Drop for LoopbackGuard {
    fn drop(&mut self) {
        set_allow_loopback(false);
    }
}

fn test_config() -> Config {
    Config {
        json_output: true,
        discover_sitemaps: true,
        fetch_retries: 0,
        retry_backoff_ms: 0,
        request_timeout_ms: Some(5_000),
        render_mode: RenderMode::Http,
        ..Config::default()
    }
}

/// Build a minimal sitemap XML containing `<loc>` entries for the given URLs.
fn sitemap_xml(urls: &[&str]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );
    for url in urls {
        xml.push_str(&format!("  <url><loc>{url}</loc></url>\n"));
    }
    xml.push_str("</urlset>\n");
    xml
}

/// Fixture: mock server with sitemap that contains duplicate URLs (same URLs
/// that the crawler would also discover). After migration, the engine merges
/// crawler + sitemap results and deduplicates — the CLI must NOT re-dedup.
fn setup_server_with_duplicate_sitemap(server: &MockServer) {
    let base = server.base_url();

    // Simple HTML page the crawler can find
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/page-a">A</a>
                    <a href="{base}/page-b">B</a>
                </body></html>"#
            ));
    });
    server.mock(|when, then| {
        when.method(GET).path("/page-a");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body>Page A content here for length</body></html>");
    });
    server.mock(|when, then| {
        when.method(GET).path("/page-b");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body>Page B content here for length</body></html>");
    });

    let page_a = format!("{base}/page-a");
    let page_b = format!("{base}/page-b");
    let page_c = format!("{base}/page-c");

    // robots.txt — no custom sitemaps
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body("User-agent: *\nDisallow:\n");
    });

    // Sitemap contains page-a and page-b (duplicates of crawler links) + page-c (new)
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&page_a, &page_b, &page_c]));
    });

    // Other default sitemap paths return 404
    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });
}

/// Contract: URLs in the output must be unique — no duplicates, stable sorted
/// order. After migration, the engine handles dedup; CLI must not re-sort/dedup.
///
/// This test constructs a fixture where sitemap URLs overlap with crawler-
/// discovered URLs. The output `urls` array must contain each URL exactly once.
#[tokio::test]
#[serial]
async fn map_payload_returns_unique_urls_without_cli_side_dedup() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    setup_server_with_duplicate_sitemap(&server);

    let base = server.base_url();
    let cfg = test_config();

    let result = map_payload(&cfg, &base)
        .await
        .expect("map_payload should not error");

    let urls = result["urls"]
        .as_array()
        .expect("urls field must be an array");

    // Collect as strings for inspection
    let url_strings: Vec<&str> = urls
        .iter()
        .map(|v| v.as_str().expect("each URL must be a string"))
        .collect();

    // Contract: no duplicates
    let mut deduped = url_strings.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        url_strings.len(),
        deduped.len(),
        "output URLs must be unique — found duplicates in: {url_strings:?}"
    );

    // Contract: sorted order (stable, deterministic output)
    let mut sorted = url_strings.clone();
    sorted.sort();
    assert_eq!(
        url_strings, sorted,
        "output URLs must be in sorted order for deterministic output"
    );

    // Sanity: must have at least the sitemap-only URL (page-c)
    let page_c = format!("{base}/page-c");
    assert!(
        url_strings.contains(&page_c.as_str()),
        "expected sitemap-only URL {page_c} in output, got: {url_strings:?}"
    );
}

/// Contract: `sitemap_urls` must equal `mapped_urls - pages_seen`, and all
/// three fields must be present and consistent in the JSON output.
#[tokio::test]
#[serial]
async fn map_payload_reports_sitemap_url_count_consistently() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    setup_server_with_duplicate_sitemap(&server);

    let base = server.base_url();
    let cfg = test_config();

    let result = map_payload(&cfg, &base)
        .await
        .expect("map_payload should not error");

    // All three fields must be present
    let mapped_urls = result["mapped_urls"]
        .as_u64()
        .expect("mapped_urls must be a number");
    let sitemap_urls = result["sitemap_urls"]
        .as_u64()
        .expect("sitemap_urls must be a number");
    let pages_seen = result["pages_seen"]
        .as_u64()
        .expect("pages_seen must be a number");

    // Contract: sitemap_urls = mapped_urls - pages_seen
    assert_eq!(
        sitemap_urls,
        mapped_urls.saturating_sub(pages_seen),
        "sitemap_urls ({sitemap_urls}) must equal mapped_urls ({mapped_urls}) - pages_seen ({pages_seen})"
    );

    // Contract: mapped_urls must equal the length of the urls array
    let urls_len = result["urls"]
        .as_array()
        .expect("urls must be an array")
        .len() as u64;
    assert_eq!(
        mapped_urls, urls_len,
        "mapped_urls ({mapped_urls}) must match urls array length ({urls_len})"
    );

    // Contract: url field must be the start URL
    assert_eq!(
        result["url"].as_str().expect("url must be a string"),
        base,
        "url field must be the start URL"
    );

    // Contract: elapsed_ms must be present and non-negative
    assert!(
        result["elapsed_ms"].as_u64().is_some(),
        "elapsed_ms must be present as a number"
    );

    // Contract: thin_pages must be present
    assert!(
        result["thin_pages"].as_u64().is_some(),
        "thin_pages must be present as a number"
    );
}

/// Contract: AutoSwitch only falls back to Chrome when `pages_seen == 0`.
/// If HTTP mode discovers at least 1 page, Chrome fallback must NOT trigger.
///
/// This test uses `RenderMode::AutoSwitch` with a server that returns at
/// least one valid HTML page. Since Chrome is not available in test, a
/// fallback attempt would either error or produce different results. We
/// verify that the output reflects the HTTP crawl (pages_seen > 0 means
/// no Chrome retry occurred).
#[tokio::test]
#[serial]
async fn map_autoswitch_only_falls_back_when_no_pages_seen() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    // Serve a page that the HTTP crawl can discover — pages_seen should be > 0
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <p>Enough content here to avoid thin-page filtering by having sufficient character length in the body.</p>
                    <a href="{base}/docs">Docs</a>
                </body></html>"#
            ));
    });
    server.mock(|when, then| {
        when.method(GET).path("/docs");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body><p>Documentation page with enough content to pass the minimum character threshold for non-thin classification.</p></body></html>");
    });

    // robots.txt + sitemap
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body("User-agent: *\nDisallow:\n");
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&format!("{base}/"), &format!("{base}/docs")]));
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });

    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..test_config()
    };

    let result = map_payload(&cfg, &base)
        .await
        .expect("map_payload should not error");

    let pages_seen = result["pages_seen"]
        .as_u64()
        .expect("pages_seen must be a number");

    // The HTTP crawl should have seen at least 1 page from the mock server.
    // If pages_seen > 0, AutoSwitch must NOT have fallen back to Chrome.
    // (If it did fall back, the test would likely error since Chrome is
    // unavailable, or pages_seen would reset to 0.)
    assert!(
        pages_seen > 0,
        "HTTP crawl over mock server should discover at least 1 page, got pages_seen={pages_seen}. \
         If this is 0, AutoSwitch may have incorrectly triggered Chrome fallback."
    );

    // Verify the output contains URLs — proves HTTP results were kept
    let urls = result["urls"].as_array().expect("urls must be an array");
    assert!(
        !urls.is_empty(),
        "output must contain URLs from the HTTP crawl"
    );
}

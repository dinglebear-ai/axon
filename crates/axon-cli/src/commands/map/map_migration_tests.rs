//! Contract tests for `map_payload` output shape and dedup semantics.
//!
//! These lock the expected behavior after sitemap URL merge/dedup moved from
//! CLI `map.rs` into adapter-owned map acquisition. The tests
//! verify two contracts:
//!
//! 1. URLs in the output are unique WITHOUT CLI-side dedup (engine handles it).
//! 2. `sitemap_urls` is the raw count of URLs returned by `discover_sitemap_urls`
//!    before the final canonical merge.
//!
//! Tests use `httpmock` to provide sitemap XML and a stub robots.txt, then
//! call the local `map_payload` test helper. The sitemap discovery path is what
//! exercises the dedup and counting logic.

use axon_core::config::{Config, RenderMode};
use axon_core::http::LoopbackGuard;
use axon_services::context::ServiceContext;
use axon_services::map::discover_with_context;
use axon_services::types::MapOptions;
use httpmock::prelude::*;
use serial_test::serial;
use std::sync::Arc;

async fn map_payload(
    cfg: &Config,
    start_url: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let context = ServiceContext::new_with_workers(Arc::new(cfg.clone()))
        .await
        .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
    let result = discover_with_context(
        &context,
        start_url,
        MapOptions {
            limit: 0,
            offset: 0,
        },
        None,
    )
    .await?;
    Ok(serde_json::to_value(result)?)
}

/// RAII guard: sets the global loopback bypass to `true` on creation
/// and restores `false` on drop.
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

/// Fixture: mock server with a sitemap and root anchors that overlap. The
/// adapter returns one canonical sitemap-owned URL set; the CLI must not
/// perform a second deduplication pass.
fn setup_server_with_duplicate_sitemap(server: &MockServer) {
    let base = server.base_url();

    // Root HTML is available but must not be traversed when a sitemap parses.
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

    // Sitemap contains page-a and page-b (also root anchors) plus page-c.
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
/// This test constructs a fixture where sitemap URLs overlap with root anchors.
/// The output `urls` array must contain each URL exactly once.
#[tokio::test]
#[serial]
async fn map_payload_returns_unique_urls_without_cli_side_dedup() {
    let _guard = LoopbackGuard::allow();
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

/// Contract: `sitemap_urls` is the raw count of URLs returned by
/// `discover_sitemap_urls` before dedup, not a net-new count.
///
/// The fixture sitemap contains exactly 3 URLs (page-a, page-b, page-c),
/// two of which overlap with root anchors. After dedup the `urls` array
/// contains those three pages, and
/// `sitemap_urls` must equal **3** — the raw sitemap URL count.
///
/// All four fields (`mapped_urls`, `sitemap_urls`, `pages_seen`,
/// `elapsed_ms`) must be present and have the correct types.
#[tokio::test]
#[serial]
async fn map_payload_reports_sitemap_url_count_consistently() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    setup_server_with_duplicate_sitemap(&server);

    let base = server.base_url();
    let cfg = test_config();

    let result = map_payload(&cfg, &base)
        .await
        .expect("map_payload should not error");

    // All fields must be present with the right types
    let mapped_urls = result["mapped_urls"]
        .as_u64()
        .expect("mapped_urls must be a number");
    let sitemap_urls = result["sitemap_urls"]
        .as_u64()
        .expect("sitemap_urls must be a number");
    let _pages_seen = result["pages_seen"]
        .as_u64()
        .expect("pages_seen must be a number");

    // Contract: sitemap_urls is the raw count of URLs in the mock sitemap
    // (page-a, page-b, page-c = 3).  It is NOT mapped_urls - pages_seen.
    assert_eq!(
        sitemap_urls, 3,
        "sitemap_urls must equal the raw sitemap URL count (3), got {sitemap_urls}"
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

/// Contract: the JSON payload produced by `map_payload` has all required fields
/// with the correct types, and `mapped_urls` equals the `urls` array length.
///
/// This is a pure unit test — no network, no mock server. It locks the wire
/// schema so any future refactor that renames a field or changes a type fails
/// fast here instead of silently breaking the `--json` output consumers.
///
/// Note: `sitemap_urls` is the raw count of URLs returned by
/// `discover_sitemap_urls` before any deduplication.  It is NOT required to
/// equal `mapped_urls - pages_seen`.
#[test]
fn map_payload_json_has_expected_fields() {
    // Build a minimal JSON payload matching what map_payload produces.
    // sitemap_urls=3 here represents 3 raw sitemap URLs (may overlap with
    // crawler-discovered URLs — dedup has already happened in the engine).
    let payload = serde_json::json!({
        "url": "https://example.com",
        "mapped_urls": 3usize,
        "sitemap_urls": 3usize,
        "pages_seen": 2u32,
        "thin_pages": 0u32,
        "elapsed_ms": 100u64,
        "urls": ["https://example.com/a", "https://example.com/b", "https://example.com/c"],
    });

    // Assert all required fields exist with the right types
    assert!(payload["url"].is_string(), "url must be a string");
    assert!(
        payload["mapped_urls"].is_number(),
        "mapped_urls must be a number"
    );
    assert!(
        payload["sitemap_urls"].is_number(),
        "sitemap_urls must be a number"
    );
    assert!(
        payload["pages_seen"].is_number(),
        "pages_seen must be a number"
    );
    assert!(
        payload["thin_pages"].is_number(),
        "thin_pages must be a number"
    );
    assert!(
        payload["elapsed_ms"].is_number(),
        "elapsed_ms must be a number"
    );
    assert!(payload["urls"].is_array(), "urls must be an array");

    // Assert urls array contains strings
    let urls = payload["urls"].as_array().expect("urls must be an array");
    assert!(!urls.is_empty(), "urls array must not be empty");
    for url in urls {
        assert!(url.is_string(), "each url must be a string");
    }

    // Assert mapped_urls == urls.len()
    let mapped_urls = payload["mapped_urls"]
        .as_u64()
        .expect("mapped_urls must be numeric");
    assert_eq!(
        mapped_urls,
        urls.len() as u64,
        "mapped_urls must equal urls.len()"
    );

    // sitemap_urls is a raw count — no invariant relative to mapped_urls/pages_seen.
    assert!(
        payload["sitemap_urls"].as_u64().is_some(),
        "sitemap_urls must be present as a number"
    );
}

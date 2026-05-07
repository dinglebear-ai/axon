//! Contract tests for the engine's `append_sitemap_backfill()` function.
//!
//! These tests exercise `append_sitemap_backfill` directly against a mock
//! HTTP server to verify sitemap discovery, deduplication, and manifest
//! generation. `sync_crawl` delegates to this function — these tests
//! validate the engine contract that `sync_crawl` relies on.
//!
//! Each test uses a global `ALLOW_LOOPBACK` bypass (via `LoopbackGuard`)
//! to permit `validate_url` to accept 127.0.0.1 — required for httpmock.
//! Tests are serialized via `#[serial]` to prevent races with SSRF tests
//! that assert loopback is blocked.

use crate::core::config::Config;
use crate::core::http::set_allow_loopback;
use crate::crawl::engine::append_sitemap_backfill;
use crate::crawl::manifest::{ManifestEntry, read_manifest_data};
use httpmock::prelude::*;
use serial_test::serial;
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::TempDir;

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

fn test_config(output_dir: PathBuf) -> Config {
    Config {
        fetch_retries: 0,
        retry_backoff_ms: 0,
        request_timeout_ms: Some(5_000),
        discover_sitemaps: true,
        drop_thin_markdown: true,
        min_markdown_chars: 200,
        embed: false,
        output_dir,
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

/// Verifies that `append_sitemap_backfill` discovers sitemap URLs and
/// updates the `CrawlSummary` with backfill metrics (discovered_urls,
/// markdown_files). This is the engine contract that `sync_crawl` relies on.
#[tokio::test]
#[serial]
async fn sync_crawl_uses_engine_backfill_metrics_not_cli_loop() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();
    let tmp = TempDir::new().expect("tempdir");
    let output_dir = tmp.path().join("output");

    let page1 = format!("{base}/docs/getting-started");
    let page2 = format!("{base}/docs/api-reference");

    // robots.txt declares a sitemap
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(format!("User-agent: *\nSitemap: {base}/sitemap.xml\n"));
    });

    // Sitemap with two pages
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&page1, &page2]));
    });

    // Default alternates return 404
    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });

    // Serve the actual pages with enough content to pass thin check
    let long_content = "a]".repeat(200);
    let html = format!("<html><body><p>{long_content}</p></body></html>");
    server.mock(|when, then| {
        when.method(GET).path("/docs/getting-started");
        then.status(200)
            .header("content-type", "text/html")
            .body(html.clone());
    });
    server.mock(|when, then| {
        when.method(GET).path("/docs/api-reference");
        then.status(200)
            .header("content-type", "text/html")
            .body(html.clone());
    });

    let cfg = test_config(output_dir.clone());

    // The engine's append_sitemap_backfill should return a summary with
    // sitemap-specific metrics (discovered URLs, backfilled count, etc.)
    // that sync_crawl can fold into its final CrawlSummary.
    let seen_urls: HashSet<String> = HashSet::new();
    let mut summary = crate::crawl::engine::CrawlSummary::default();

    let backfill_result =
        append_sitemap_backfill(&cfg, &base, &output_dir, &seen_urls, &mut summary)
            .await
            .expect("engine backfill should succeed");

    // Contract: the engine backfill reports how many sitemap URLs it discovered.
    assert!(
        backfill_result.discovered_urls > 0,
        "engine backfill must report discovered_urls > 0, got: {}",
        backfill_result.discovered_urls
    );

    // Contract: the engine backfill writes markdown files and updates the summary.
    assert!(
        summary.markdown_files > 0,
        "engine backfill must update summary.markdown_files, got: {}",
        summary.markdown_files
    );
}

/// Verifies that `append_sitemap_backfill` does not duplicate manifest rows
/// for URLs already present in the seen set, and that new entries use
/// `changed: true`.
#[tokio::test]
#[serial]
async fn sync_crawl_does_not_append_manifest_via_cli_backfill_codepath() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();
    let tmp = TempDir::new().expect("tempdir");
    let output_dir = tmp.path().join("output");
    tokio::fs::create_dir_all(&output_dir)
        .await
        .expect("create output dir");

    let page1 = format!("{base}/docs/page1");
    let page2 = format!("{base}/docs/page2");

    // robots.txt declares a sitemap
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(format!("User-agent: *\nSitemap: {base}/sitemap.xml\n"));
    });

    // Sitemap with two pages — page1 is already "seen" (crawled), page2 is new
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&page1, &page2]));
    });

    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });

    // Serve page2 with enough content
    let long_content = "b".repeat(300);
    let html = format!("<html><body><p>{long_content}</p></body></html>");
    server.mock(|when, then| {
        when.method(GET).path("/docs/page2");
        then.status(200)
            .header("content-type", "text/html")
            .body(html.clone());
    });

    let cfg = test_config(output_dir.clone());

    // Pre-populate seen_urls with page1 — simulating it was already crawled
    let mut seen_urls: HashSet<String> = HashSet::new();
    seen_urls.insert(page1.clone());

    let mut summary = crate::crawl::engine::CrawlSummary::default();

    let _backfill_result =
        append_sitemap_backfill(&cfg, &base, &output_dir, &seen_urls, &mut summary)
            .await
            .expect("engine backfill should succeed");

    // Contract: page1 was already seen, so only page2 should be backfilled.
    // The manifest must not contain duplicate entries for page1.
    let manifest_path = output_dir.join("manifest.jsonl");
    let manifest = read_manifest_data(&manifest_path)
        .await
        .expect("read manifest");

    let page1_entries: Vec<&ManifestEntry> = manifest.values().filter(|e| e.url == page1).collect();
    assert!(
        page1_entries.is_empty(),
        "page1 was already seen — engine backfill must not create a manifest entry for it, found: {page1_entries:?}"
    );

    let page2_entries: Vec<&ManifestEntry> = manifest.values().filter(|e| e.url == page2).collect();
    assert_eq!(
        page2_entries.len(),
        1,
        "page2 is new — engine backfill must create exactly one manifest entry, found: {}",
        page2_entries.len()
    );

    // Contract: engine-backfilled entries use `changed: true` since they are
    // new to this crawl session (same semantics as the old CLI path).
    assert!(
        page2_entries[0].changed,
        "engine-backfilled manifest entries must have changed=true"
    );
}

//! Targeted tests for sitemap-first map behavior.
//!
//! Tests cover the 3-stage fallback chain:
//!   sitemap discovery → bounded-structure fallback → crawl (opt-in only)
//!
//! All tests use httpmock for network isolation.

use super::map_payload;
use crate::crates::core::config::{Config, MapFallback, RenderMode};
use crate::crates::core::http::set_allow_loopback;
use httpmock::prelude::*;
use serial_test::serial;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

fn base_config() -> Config {
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

fn sitemap_index_xml(child_urls: &[&str]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
"#,
    );
    for url in child_urls {
        xml.push_str(&format!("  <sitemap><loc>{url}</loc></sitemap>\n"));
    }
    xml.push_str("</sitemapindex>\n");
    xml
}

/// Register all default sitemap seed paths as 404, except the one being tested.
fn mock_all_sitemaps_404(server: &MockServer) {
    for path in &[
        "/sitemap.xml",
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
}

// ---------------------------------------------------------------------------
// Test 1: Sitemap-first behavior — robots.txt → sitemap.xml → 10 URLs
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_sitemap_first_uses_sitemap_urls() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    let page_urls: Vec<String> = (1..=10).map(|i| format!("{base}/page-{i}")).collect();
    let page_url_refs: Vec<&str> = page_urls.iter().map(String::as_str).collect();

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(format!(
                "User-agent: *\nDisallow:\nSitemap: {base}/sitemap.xml\n"
            ));
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&page_url_refs));
    });
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    let urls = result["urls"].as_array().expect("urls must be array");
    assert_eq!(
        urls.len(),
        10,
        "expected 10 sitemap URLs, got {}: {:?}",
        urls.len(),
        urls
    );
    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap"),
        "expected map_source=sitemap"
    );
    // pages_seen must be 0 in sitemap mode
    assert_eq!(
        result["pages_seen"].as_u64(),
        Some(0),
        "pages_seen must be 0 in sitemap mode"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Fallback trigger correctness — sitemap parsed but all out-of-scope
//         → map_source stays "sitemap", no anchor fallback
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_out_of_scope_sitemap_no_anchor_fallback() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    // Sitemap exists and is parsed, but URLs are all on a different host.
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[
                "https://other.example.com/page1",
                "https://other.example.com/page2",
            ]));
    });
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    // map_source must be "sitemap" — sitemap was parsed even though all URLs were out-of-scope.
    // The fallback trigger is parsed_sitemap_documents == 0, not urls.len() == 0.
    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap"),
        "map_source must be 'sitemap' when sitemap was parsed but all URLs out-of-scope"
    );
    // URLs must be empty (all filtered out)
    let urls = result["urls"].as_array().expect("urls must be array");
    assert_eq!(
        urls.len(),
        0,
        "expected 0 URLs after scope filtering, got {urls:?}"
    );
    // warning must be null — no anchor fallback was triggered
    assert!(
        result["warning"].is_null(),
        "warning must be null when sitemap was parsed (no anchor fallback)"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Bounded structure fallback — no sitemap → root page anchors
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_bounded_structure_fallback_uses_anchor_hrefs() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    mock_all_sitemaps_404(&server);

    // Root page with internal anchor links
    let link_html = (1..=10)
        .map(|i| format!(r#"<a href="{base}/section-{i}">Section {i}</a>"#))
        .collect::<Vec<_>>()
        .join("\n");
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!("<html><body>{link_html}</body></html>"));
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("bounded-structure"),
        "expected map_source=bounded-structure when no sitemaps found"
    );
    let urls = result["urls"].as_array().expect("urls must be array");
    assert!(
        !urls.is_empty(),
        "expected anchor URLs in bounded-structure mode, got empty"
    );
    // Verify the URLs are from the mock server host
    for url in urls {
        let u = url.as_str().expect("url must be a string");
        assert!(
            u.contains("127.0.0.1") || u.contains("localhost"),
            "expected internal URL, got: {u}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 4: No-crawl lock-in — bounded-structure mode does NOT invoke Spider
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_structure_mode_does_not_crawl() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    mock_all_sitemaps_404(&server);

    // Root page with some links
    let root_mock = server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/doc1">Doc 1</a>
                    <a href="{base}/doc2">Doc 2</a>
                    <a href="{base}/doc3">Doc 3</a>
                    <a href="{base}/doc4">Doc 4</a>
                    <a href="{base}/doc5">Doc 5</a>
                </body></html>"#
            ));
    });

    // These pages must NOT be fetched in bounded-structure mode.
    // If Spider crawls, it would fetch these — track hits to detect crawling.
    let deep_mock = server.mock(|when, then| {
        when.method(GET).path_matches(r"^/doc\d+$");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body>deep page content</body></html>");
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("bounded-structure"),
        "expected bounded-structure"
    );
    // Root was fetched once for anchor extraction
    assert!(root_mock.calls() >= 1, "root page should be fetched");
    // Deep pages must NOT be fetched — Spider crawl was NOT triggered
    assert_eq!(
        deep_mock.calls(),
        0,
        "deep pages must NOT be fetched in bounded-structure mode (no Spider crawl)"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Sitemap index recursion — child sitemaps resolved and included
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_sitemap_index_recursion() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });

    // Root sitemap is a sitemap index pointing to two child sitemaps
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_index_xml(&[
                &format!("{base}/sitemap-1.xml"),
                &format!("{base}/sitemap-2.xml"),
            ]));
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-1.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&format!("{base}/a"), &format!("{base}/b")]));
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-2.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&format!("{base}/c"), &format!("{base}/d")]));
    });
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap"),
        "expected sitemap source"
    );
    let urls = result["urls"].as_array().expect("urls must be array");
    assert!(
        urls.len() >= 4,
        "expected at least 4 URLs from child sitemaps, got {}: {:?}",
        urls.len(),
        urls
    );
}

// ---------------------------------------------------------------------------
// Test 6: Scoping — out-of-host URLs filtered from sitemap
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_sitemap_out_of_host_urls_filtered() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[
                &format!("{base}/in-scope"),
                "https://different-host.example.com/out-of-scope",
                "https://evil.example.com/also-out",
            ]));
    });
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    let urls: Vec<&str> = result["urls"]
        .as_array()
        .expect("urls must be array")
        .iter()
        .map(|v| v.as_str().expect("url must be string"))
        .collect();

    // Out-of-host URLs must not appear
    assert!(
        !urls
            .iter()
            .any(|u| u.contains("different-host.example.com")),
        "out-of-host URL must be filtered: {urls:?}"
    );
    assert!(
        !urls.iter().any(|u| u.contains("evil.example.com")),
        "out-of-host URL must be filtered: {urls:?}"
    );
    // In-scope URL must appear
    let in_scope = format!("{base}/in-scope");
    assert!(
        urls.contains(&in_scope.as_str()),
        "in-scope URL must be present: {urls:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 7: --map-fallback crawl opt-in triggers crawl when no sitemap
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_map_fallback_crawl_opt_in() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    mock_all_sitemaps_404(&server);

    // Root page with links
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/doc-a">Doc A</a>
                    <a href="{base}/doc-b">Doc B</a>
                </body></html>"#
            ));
    });
    server.mock(|when, then| {
        when.method(GET).path("/doc-a");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body>Doc A content that is long enough to pass thin check</body></html>");
    });
    server.mock(|when, then| {
        when.method(GET).path("/doc-b");
        then.status(200)
            .header("content-type", "text/html")
            .body("<html><body>Doc B content that is long enough to pass thin check</body></html>");
    });

    let cfg = Config {
        map_fallback: MapFallback::Crawl,
        ..base_config()
    };
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("crawl"),
        "expected map_source=crawl when --map-fallback crawl is set"
    );
}

// ---------------------------------------------------------------------------
// Test 8: Security — cross-origin anchor URLs NOT in bounded-structure output
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_bounded_structure_cross_origin_filtered() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    mock_all_sitemaps_404(&server);

    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/safe-page">Safe</a>
                    <a href="https://other.example.com/external">External</a>
                    <a href="https://evil.example.com/phish">Evil</a>
                </body></html>"#
            ));
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    let urls: Vec<&str> = result["urls"]
        .as_array()
        .expect("urls must be array")
        .iter()
        .map(|v| v.as_str().expect("url must be string"))
        .collect();

    // Cross-origin URLs must not appear
    assert!(
        !urls.iter().any(|u| u.contains("other.example.com")),
        "cross-origin URL must not appear in bounded-structure output: {urls:?}"
    );
    assert!(
        !urls.iter().any(|u| u.contains("evil.example.com")),
        "cross-origin URL must not appear in bounded-structure output: {urls:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 9: pages_seen = 0 in sitemap mode
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_pages_seen_zero_in_sitemap_mode() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[
                &format!("{base}/a"),
                &format!("{base}/b"),
                &format!("{base}/c"),
            ]));
    });
    for path in &[
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ] {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        });
    }

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap"),
        "expected sitemap source"
    );
    assert_eq!(
        result["pages_seen"].as_u64(),
        Some(0),
        "pages_seen must be 0 in sitemap mode (no pages crawled)"
    );
}

// ---------------------------------------------------------------------------
// Test 10: warning field set when bounded-structure returns fewer than 5 URLs
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_warning_when_bounded_structure_too_few_urls() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    mock_all_sitemaps_404(&server);

    // Root page with only 3 internal links — fewer than 5
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/p1">P1</a>
                    <a href="{base}/p2">P2</a>
                    <a href="{base}/p3">P3</a>
                </body></html>"#
            ));
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("bounded-structure"),
        "expected bounded-structure"
    );
    // warning must be non-null when fewer than 5 URLs found
    assert!(
        result["warning"].is_string(),
        "warning must be a non-null string when bounded-structure returns < 5 URLs, got: {}",
        result["warning"]
    );
    let warning_text = result["warning"].as_str().unwrap();
    assert!(
        warning_text.contains("crawl") || warning_text.contains("SPA"),
        "warning should suggest using --map-fallback crawl: {warning_text}"
    );
}

// ---------------------------------------------------------------------------
// Test 11: --discover-sitemaps false skips sitemap fetch entirely
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_discover_sitemaps_false_skips_sitemap_fetch() {
    let _guard = LoopbackGuard::new();
    let server = MockServer::start();
    let base = server.base_url();

    // robots.txt + every sitemap path is mocked. If the gate works, none of
    // these should be hit — we assert calls() == 0 below.
    let robots_mock = server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(format!(
                "User-agent: *\nDisallow:\nSitemap: {base}/sitemap.xml\n"
            ));
    });
    let sitemap_mock = server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[
                &format!("{base}/from-sitemap-1"),
                &format!("{base}/from-sitemap-2"),
            ]));
    });
    let other_sitemap_mocks: Vec<_> = [
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ]
    .iter()
    .map(|path| {
        server.mock(|when, then| {
            when.method(GET).path(*path);
            then.status(404);
        })
    })
    .collect();

    // Root page: bounded-structure fallback should fetch this and extract anchors.
    server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                r#"<html><body>
                    <a href="{base}/from-anchor-1">A1</a>
                    <a href="{base}/from-anchor-2">A2</a>
                    <a href="{base}/from-anchor-3">A3</a>
                    <a href="{base}/from-anchor-4">A4</a>
                    <a href="{base}/from-anchor-5">A5</a>
                </body></html>"#
            ));
    });

    let cfg = Config {
        discover_sitemaps: false,
        ..base_config()
    };
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    // Sitemap discovery must be skipped entirely — no fetches to robots.txt or
    // any sitemap path.
    assert_eq!(
        robots_mock.calls(),
        0,
        "robots.txt must NOT be fetched when --discover-sitemaps false"
    );
    assert_eq!(
        sitemap_mock.calls(),
        0,
        "sitemap.xml must NOT be fetched when --discover-sitemaps false"
    );
    for m in &other_sitemap_mocks {
        assert_eq!(
            m.calls(),
            0,
            "no sitemap path should be fetched when --discover-sitemaps false"
        );
    }

    // Bounded-structure fallback must take over.
    assert_eq!(
        result["map_source"].as_str(),
        Some("bounded-structure"),
        "expected bounded-structure when --discover-sitemaps false"
    );
    let urls = result["urls"].as_array().expect("urls must be array");
    assert!(
        !urls.is_empty(),
        "bounded-structure should have produced anchor URLs, got empty"
    );
    // Sanity: URLs come from anchor extraction, not sitemap.
    let url_strs: Vec<&str> = urls.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        url_strs.iter().any(|u| u.contains("from-anchor-")),
        "expected anchor-derived URLs, got: {url_strs:?}"
    );
    assert!(
        !url_strs.iter().any(|u| u.contains("from-sitemap-")),
        "sitemap URLs must NOT appear when discovery is disabled: {url_strs:?}"
    );
}

//! Characterization tests for `discover_sitemap_urls_with_robots`.
//!
//! These lock the current behavior of the CLI sitemap discovery function
//! before any migration to the engine-backed adapter.
//!
//! Each test uses a global `ALLOW_LOOPBACK` bypass (via `LoopbackGuard`)
//! to permit `validate_url` to accept 127.0.0.1 — required for httpmock.
//! Tests are serialized via `#[serial]` to prevent races with SSRF tests
//! that assert loopback is blocked.

use super::sitemap::discover_sitemap_urls_with_robots;
use crate::core::config::Config;
use crate::core::http::LoopbackGuard;
use httpmock::prelude::*;
use serial_test::serial;

/// RAII guard: sets the global loopback bypass to `true` on creation
/// and restores `false` on drop.
fn test_config() -> Config {
    Config {
        fetch_retries: 0,
        retry_backoff_ms: 0,
        request_timeout_ms: Some(5_000),
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

/// When robots.txt declares a custom sitemap URL, the discovered URLs must
/// include entries from that declared sitemap.
#[tokio::test]
#[serial]
async fn discover_sitemap_urls_includes_robots_declared_entries() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url(); // http://127.0.0.1:PORT

    let page1 = format!("{base}/docs/getting-started");
    let page2 = format!("{base}/docs/api-reference");

    // robots.txt declares a custom sitemap path
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(format!(
                "User-agent: *\nSitemap: {base}/custom-sitemap.xml\n"
            ));
    });

    // Default sitemap paths return 404
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });

    // Custom sitemap declared in robots.txt
    server.mock(|when, then| {
        when.method(GET).path("/custom-sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&page1, &page2]));
    });

    let cfg = test_config();
    let result = discover_sitemap_urls_with_robots(&cfg, &base)
        .await
        .expect("discovery should not error");

    // robots.txt declared 1 sitemap
    assert_eq!(
        result.stats.robots_declared_sitemaps, 1,
        "expected 1 robots-declared sitemap"
    );

    // Both pages from the custom sitemap must be present
    assert!(
        result.urls.contains(&page1),
        "expected {page1} in discovered URLs, got: {:?}",
        result.urls
    );
    assert!(
        result.urls.contains(&page2),
        "expected {page2} in discovered URLs, got: {:?}",
        result.urls
    );
}

/// URLs matching `exclude_path_prefix` must be filtered out.
#[tokio::test]
#[serial]
async fn discover_sitemap_urls_applies_exclude_path_prefix() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url();

    let en_page = format!("{base}/docs/en/page1");
    let ja_page = format!("{base}/docs/ja/page2");

    // robots.txt — no custom sitemaps
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body("User-agent: *\nDisallow:\n");
    });

    // Primary sitemap with both en and ja pages
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&en_page, &ja_page]));
    });

    // Other default sitemaps return 404
    server.mock(|when, then| {
        when.method(GET).path("/sitemap_index.xml");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap-index.xml");
        then.status(404);
    });

    let cfg = Config {
        exclude_path_prefix: vec!["/docs/ja".into()],
        ..test_config()
    };

    let result = discover_sitemap_urls_with_robots(&cfg, &base)
        .await
        .expect("discovery should not error");

    // English page included
    assert!(
        result.urls.contains(&en_page),
        "expected {en_page} in discovered URLs, got: {:?}",
        result.urls
    );

    // Japanese page excluded by prefix
    assert!(
        !result.urls.contains(&ja_page),
        "expected {ja_page} to be excluded, got: {:?}",
        result.urls
    );

    // The engine filters internally — the stats may report 0 for granular
    // filter counters, but the filtering itself must still work.
}

/// With `include_subdomains=false`, only URLs on the exact start host should
/// be returned — subdomain URLs must be filtered out.
#[tokio::test]
#[serial]
async fn discover_sitemap_urls_respects_include_subdomains_false() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url(); // http://127.0.0.1:PORT

    let main_page = format!("{base}/docs/intro");
    // These use different hostnames that won't match the start URL's host
    let docs_page = "http://docs.example.com/guide/setup".to_string();
    let blog_page = "http://blog.example.com/post/1".to_string();

    // robots.txt — no custom sitemaps
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body("User-agent: *\nDisallow:\n");
    });

    // Sitemap includes URLs from the main host + two subdomains
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&main_page, &docs_page, &blog_page]));
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
        include_subdomains: false,
        ..test_config()
    };

    let result = discover_sitemap_urls_with_robots(&cfg, &base)
        .await
        .expect("discovery should not error");

    // Main host page included
    assert!(
        result.urls.contains(&main_page),
        "expected {main_page} in discovered URLs, got: {:?}",
        result.urls
    );

    // Subdomain pages excluded
    assert!(
        !result.urls.contains(&docs_page),
        "expected docs.example.com URL to be excluded, got: {:?}",
        result.urls
    );
    assert!(
        !result.urls.contains(&blog_page),
        "expected blog.example.com URL to be excluded, got: {:?}",
        result.urls
    );
}

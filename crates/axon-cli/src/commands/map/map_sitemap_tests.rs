//! Sitemap-layer map tests: robots.txt -> sitemap.xml -> scoped URLs,
//! sitemapindex recursion, host scoping, and the thin-sitemap fallback gate.
//!
//! Anchor and llms.txt layers live in `map_fallback_tests`.
//! Shared fixtures live in `map_test_support`.

use super::map_test_support::*;
use axon_core::config::Config;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_sitemap_first_uses_sitemap_urls() {
    let _guard = LoopbackGuard::allow();
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
    mock_index_seeds_404(&server);

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
// Test 2: Fallback trigger correctness — a sitemap that parsed but yielded too
//         few in-scope URLs must STILL fall through to anchor discovery, and
//         the two layers must merge.
//
// Regression: the trigger used to be `parsed_sitemap_documents == 0`, so a
// sitemap listing only out-of-scope hosts returned 0 URLs with a null warning
// and never reached anchors. www.lex-co.sc.gov mapped 0 URLs this way — its
// sitemap lists a single apex URL while the crawl was seeded from `www.`.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_out_of_scope_sitemap_falls_back_to_anchors() {
    let _guard = LoopbackGuard::allow();
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
    mock_index_seeds_404(&server);

    // Root page carries in-scope anchors the sitemap never advertised.
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

    // Both layers ran, so the source names both.
    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap+bounded-structure"),
        "a thin/out-of-scope sitemap must still reach anchor discovery"
    );
    let urls = result["urls"].as_array().expect("urls must be array");
    assert!(
        !urls.is_empty(),
        "anchors must be recovered when the sitemap yields nothing in scope"
    );
    for url in urls {
        let u = url.as_str().expect("url must be a string");
        assert!(
            u.contains("127.0.0.1") || u.contains("localhost"),
            "out-of-scope sitemap host must not leak into results, got: {u}"
        );
    }
    assert!(
        result["warning"].is_null(),
        "no warning expected once anchors supplied a healthy map, got {:?}",
        result["warning"]
    );
}

#[tokio::test]
#[serial]
async fn test_thin_sitemap_still_reaches_anchor_discovery() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url();

    // A parsed, in-scope, but THIN sitemap: one URL. Previously this returned
    // immediately with a single URL; anchors must now top it up.
    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[&format!("{base}/only-known-page")]));
    });
    mock_index_seeds_404(&server);

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

    let urls: Vec<&str> = result["urls"]
        .as_array()
        .expect("urls must be array")
        .iter()
        .map(|u| u.as_str().expect("url must be a string"))
        .collect();

    assert!(
        urls.iter().any(|u| u.ends_with("/only-known-page")),
        "sitemap entry must survive the merge, got {urls:?}"
    );
    assert!(
        urls.iter().any(|u| u.contains("/section-")),
        "anchor URLs must be merged in, got {urls:?}"
    );
    // Merge must not duplicate.
    let mut deduped = urls.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), urls.len(), "merged map contains duplicates");
}

// ---------------------------------------------------------------------------
// Test 5: Sitemap index recursion — child sitemaps resolved and included
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_sitemap_index_recursion() {
    let _guard = LoopbackGuard::allow();
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
    mock_index_seeds_404(&server);

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
    let _guard = LoopbackGuard::allow();
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
    mock_index_seeds_404(&server);

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
// Test 9: pages_seen = 0 in sitemap mode
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_pages_seen_zero_in_sitemap_mode() {
    let _guard = LoopbackGuard::allow();
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
    mock_index_seeds_404(&server);

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
// Test 11: config discover_sitemaps=false skips sitemap fetch entirely
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_discover_sitemaps_false_skips_sitemap_fetch() {
    let _guard = LoopbackGuard::allow();
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
        "robots.txt must NOT be fetched when discover_sitemaps=false"
    );
    assert_eq!(
        sitemap_mock.calls(),
        0,
        "sitemap.xml must NOT be fetched when discover_sitemaps=false"
    );
    for m in &other_sitemap_mocks {
        assert_eq!(
            m.calls(),
            0,
            "no sitemap path should be fetched when discover_sitemaps=false"
        );
    }

    // Bounded-structure fallback must take over.
    assert_eq!(
        result["map_source"].as_str(),
        Some("bounded-structure"),
        "expected bounded-structure when discover_sitemaps=false"
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

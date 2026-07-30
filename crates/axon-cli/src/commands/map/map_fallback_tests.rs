//! Fallback-layer map tests: root-page anchor discovery, the no-crawl
//! guarantee, cross-origin filtering, and llms.txt discovery.
//!
//! Sitemap-layer tests live in `map_sitemap_tests`.
//! Shared fixtures live in `map_test_support`.

use super::map_test_support::*;
use axon_core::config::Config;
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use serial_test::serial;

// ---------------------------------------------------------------------------
// Test 3: Bounded structure fallback — no sitemap → root page anchors
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_bounded_structure_fallback_uses_anchor_hrefs() {
    let _guard = LoopbackGuard::allow();
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
    let _guard = LoopbackGuard::allow();
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
// Security — cross-origin anchor URLs NOT in bounded-structure output
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_bounded_structure_cross_origin_filtered() {
    let _guard = LoopbackGuard::allow();
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
// Warning field set when bounded-structure returns fewer than 5 URLs
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_warning_when_bounded_structure_too_few_urls() {
    let _guard = LoopbackGuard::allow();
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
        warning_text.contains("dynamic navigation"),
        "warning should explain bounded discovery limits: {warning_text}"
    );
}

// ---------------------------------------------------------------------------
// llms.txt union + dedupe into sitemap discovery
// ---------------------------------------------------------------------------

fn llms_txt_body(urls: &[&str]) -> String {
    let mut s = String::from("# Docs\n\n> Summary.\n\n## Pages\n\n");
    for url in urls {
        s.push_str(&format!("- [link]({url})\n"));
    }
    s
}

#[tokio::test]
#[serial]
async fn map_unions_sitemap_and_llms_txt_deduped() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url();

    let a = format!("{base}/a");
    let b = format!("{base}/b");
    let c = format!("{base}/c");

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[a.as_str(), b.as_str()]));
    });
    mock_index_seeds_404(&server);
    // llms.txt links /b (overlaps sitemap) and /c (new).
    server.mock(|when, then| {
        when.method(GET).path("/llms.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(llms_txt_body(&[b.as_str(), c.as_str()]));
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    let urls: Vec<String> = result["urls"]
        .as_array()
        .expect("urls must be array")
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(urls.len(), 3, "a,b,c with b deduped, got {urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/a")));
    assert!(urls.iter().any(|u| u.ends_with("/b")));
    assert!(urls.iter().any(|u| u.ends_with("/c")));
    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap+llms"),
        "map_source must reflect both sources"
    );
}

#[tokio::test]
#[serial]
async fn map_skips_llms_txt_when_disabled() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url();

    let a = format!("{base}/a");

    server.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(404);
    });
    server.mock(|when, then| {
        when.method(GET).path("/sitemap.xml");
        then.status(200)
            .header("content-type", "application/xml")
            .body(sitemap_xml(&[a.as_str()]));
    });
    mock_index_seeds_404(&server);
    // If discover_llms_txt is honored as false, this mock must never be hit.
    let llms_mock = server.mock(|when, then| {
        when.method(GET).path("/llms.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(llms_txt_body(&[
                format!("{base}/should-not-appear").as_str()
            ]));
    });

    let cfg = Config {
        discover_llms_txt: false,
        ..base_config()
    };
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        llms_mock.calls(),
        0,
        "/llms.txt must not be fetched when disabled"
    );
    assert_eq!(
        result["map_source"].as_str(),
        Some("sitemap"),
        "map_source must be plain sitemap when llms.txt disabled"
    );
}

/// llms-only branch: every sitemap path 404s (no sitemap parsed) but `/llms.txt` is valid.
/// The curated llms.txt links must survive — `map_source` is "llms" and they appear in the
/// result. Guards the early-return-drop regression where no-sitemap root-anchor discovery
/// lost the llms.txt links. `map_source:"llms"` had no test.
#[tokio::test]
#[serial]
async fn map_llms_only_when_no_sitemap() {
    let _guard = LoopbackGuard::allow();
    let server = MockServer::start();
    let base = server.base_url();

    let x = format!("{base}/x");
    let y = format!("{base}/y");

    // No sitemap anywhere (all seeds + robots 404).
    mock_all_sitemaps_404(&server);
    // Valid llms.txt with two same-host links.
    server.mock(|when, then| {
        when.method(GET).path("/llms.txt");
        then.status(200)
            .header("content-type", "text/plain")
            .body(llms_txt_body(&[x.as_str(), y.as_str()]));
    });

    let cfg = base_config();
    let result = map_payload(&cfg, &base).await.expect("map_payload failed");

    assert_eq!(
        result["map_source"].as_str(),
        Some("llms"),
        "map_source must be 'llms' when only llms.txt yields URLs"
    );
    let urls: Vec<String> = result["urls"]
        .as_array()
        .expect("urls must be array")
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        urls.iter().any(|u| u.ends_with("/x")),
        "llms.txt URL /x must be present: {urls:?}"
    );
    assert!(
        urls.iter().any(|u| u.ends_with("/y")),
        "llms.txt URL /y must be present: {urls:?}"
    );
}

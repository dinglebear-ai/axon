use super::*;
use crate::boundary::FakeAdapterProviders;
use axon_core::config::Config;

/// Unit test for `sitemap_loc_in_scope` using real domain names.
/// The integration test uses a loopback mock server (IP address) where
/// IP addresses have no subdomain relationship — this test exercises
/// the actual subdomain branching logic directly with real hostnames.
#[test]
fn sitemap_loc_in_scope_subdomain_branching() {
    let cfg_no_sub = Config {
        include_subdomains: false,
        ..Config::default()
    };
    let cfg_with_sub = Config {
        include_subdomains: true,
        ..Config::default()
    };

    // Same host: included regardless of include_subdomains setting.
    assert!(
        loc_in_scope(
            &cfg_no_sub,
            "https://docs.example.com/page",
            "docs.example.com",
            "/",
            true
        )
        .is_some(),
        "same host should always be in scope"
    );

    // Subdomain with include_subdomains=false: excluded.
    assert!(
        loc_in_scope(
            &cfg_no_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "subdomain should be excluded when include_subdomains=false"
    );

    // Subdomain with include_subdomains=true: included.
    assert!(
        loc_in_scope(
            &cfg_with_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_some(),
        "subdomain should be included when include_subdomains=true"
    );

    // Completely different domain: excluded with both settings.
    assert!(
        loc_in_scope(
            &cfg_with_sub,
            "https://other.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "unrelated domain should never be in scope"
    );
}

#[test]
fn sitemap_url_budget_caps_zero_and_oversized_page_limits() {
    let uncapped = Config {
        max_pages: 0,
        ..Config::default()
    };
    let oversized = Config {
        max_pages: u32::MAX,
        ..Config::default()
    };
    let explicit = Config {
        max_pages: 37,
        ..Config::default()
    };

    assert_eq!(
        sitemap_url_limit(&uncapped),
        super::super::MAX_TRACKED_DISCOVERED_URLS
    );
    assert_eq!(
        sitemap_url_limit(&oversized),
        super::super::MAX_TRACKED_DISCOVERED_URLS
    );
    assert_eq!(sitemap_url_limit(&explicit), 37);
}

#[test]
fn sitemap_fetch_budget_caps_zero_and_oversized_limits() {
    let uncapped = Config {
        max_sitemaps: 0,
        ..Config::default()
    };
    let oversized = Config {
        max_sitemaps: usize::MAX,
        ..Config::default()
    };
    let explicit = Config {
        max_sitemaps: 23,
        ..Config::default()
    };

    assert_eq!(
        sitemap_fetch_limit(&uncapped),
        super::super::MAX_TRACKED_DISCOVERED_URLS
    );
    assert_eq!(
        sitemap_fetch_limit(&oversized),
        super::super::MAX_TRACKED_DISCOVERED_URLS
    );
    assert_eq!(sitemap_fetch_limit(&explicit), 23);
}

#[test]
fn discovered_urls_are_rejected_at_insertion_budget() {
    let mut urls = std::collections::HashSet::new();

    assert!(insert_discovered_url(
        &mut urls,
        "https://example.com/a".to_string(),
        2
    ));
    assert!(!insert_discovered_url(
        &mut urls,
        "https://example.com/b".to_string(),
        2
    ));
    assert!(!insert_discovered_url(
        &mut urls,
        "https://example.com/c".to_string(),
        2
    ));
    assert_eq!(urls.len(), 2);
    assert!(!urls.contains("https://example.com/c"));
}

#[test]
fn markdown_url_uses_passthrough() {
    assert!(is_already_markdown("https://x.com/docs/api.md"));
    assert!(is_already_markdown("https://x.com/llms.txt"));
    assert!(is_already_markdown("https://x.com/a/b.MD")); // case-insensitive
    assert!(!is_already_markdown("https://x.com/docs/page"));
    assert!(!is_already_markdown("https://x.com/index.html"));
    // Query string is stripped before the extension check.
    assert!(is_already_markdown("https://x.com/a.md?v=1"));
    // .markdown extension is recognized alongside .md.
    assert!(is_already_markdown("https://x.com/a.markdown"));
    // Fragment is stripped before the (case-insensitive) extension check.
    assert!(is_already_markdown("https://x.com/a.MD#h"));
}

#[tokio::test]
async fn fetch_text_rejects_oversized_body() {
    let providers = FakeAdapterProviders::new().with_fetch_text("x".repeat(600 * 1024));
    let got = fetch_text(
        &providers,
        "https://example.test/big.txt",
        Some(DISCOVERY_MAX_BODY_BYTES),
    )
    .await;
    assert!(
        got.is_none(),
        "oversized provider content must be rejected before discovery buffers it"
    );
}

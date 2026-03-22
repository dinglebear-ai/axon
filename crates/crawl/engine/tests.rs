use super::*;

fn summary(pages_seen: u32, thin: u32, markdown_files: u32) -> CrawlSummary {
    CrawlSummary {
        pages_seen,
        thin_pages: thin,
        markdown_files,
        elapsed_ms: 0,
        reused_pages: 0,
        pages_discovered: 0,
        thin_urls: HashSet::new(),
        error_pages: 0,
        waf_blocked_pages: 0,
        waf_blocked_urls: HashSet::new(),
    }
}

/// Default config with standard auto-switch thresholds (0.60 ratio, 10 min pages).
fn default_cfg() -> Config {
    Config::default()
}

#[test]
fn test_fallback_when_no_markdown_files() {
    assert!(should_fallback_to_chrome(
        &summary(100, 0, 0),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_thin_ratio_above_threshold() {
    assert!(should_fallback_to_chrome(
        &summary(100, 61, 50),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_no_fallback_at_threshold() {
    assert!(!should_fallback_to_chrome(
        &summary(100, 60, 50),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_low_coverage() {
    assert!(should_fallback_to_chrome(
        &summary(100, 10, 5),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_no_divide_by_zero() {
    assert!(should_fallback_to_chrome(
        &summary(0, 0, 0),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_no_fallback_healthy_crawl() {
    assert!(!should_fallback_to_chrome(
        &summary(200, 10, 150),
        200,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_low_max_pages() {
    assert!(should_fallback_to_chrome(
        &summary(50, 5, 8),
        50,
        &default_cfg()
    ));
}

#[test]
fn test_no_fallback_small_crawl_sufficient_coverage() {
    assert!(!should_fallback_to_chrome(
        &summary(50, 5, 15),
        50,
        &default_cfg()
    ));
}

#[test]
fn test_exclude_path_prefix_matches_segment_boundary() {
    let excludes = vec!["/de".to_string()];
    assert!(is_excluded_url_path("https://example.com/de", &excludes));
    assert!(is_excluded_url_path(
        "https://example.com/de/docs",
        &excludes
    ));
    assert!(!is_excluded_url_path(
        "https://example.com/developer",
        &excludes
    ));
    assert!(!is_excluded_url_path(
        "https://example.com/design",
        &excludes
    ));
}

#[test]
fn test_exclude_path_prefix_blocks_locale_region_variants() {
    // /ja should block /ja-jp, /ja-kr, etc. (hyphen-separated region codes)
    let excludes = vec!["/ja".to_string(), "/ko".to_string()];
    assert!(is_excluded_url_path("https://example.com/ja", &excludes));
    assert!(is_excluded_url_path(
        "https://example.com/ja/docs",
        &excludes
    ));
    assert!(is_excluded_url_path(
        "https://example.com/ja-jp/blog",
        &excludes
    ));
    assert!(is_excluded_url_path(
        "https://example.com/ko-kr/blog",
        &excludes
    ));
    // Should NOT block unrelated paths that start with the same letters
    assert!(!is_excluded_url_path(
        "https://example.com/javascript",
        &excludes
    ));
    assert!(!is_excluded_url_path(
        "https://example.com/korean-food",
        &excludes
    ));
}

#[test]
fn test_exclude_path_prefix_handles_non_normalized_input() {
    let excludes = vec!["de/".to_string()];
    assert!(is_excluded_url_path("https://example.com/de", &excludes));
    assert!(is_excluded_url_path(
        "https://example.com/de/guide",
        &excludes
    ));
    assert!(!is_excluded_url_path(
        "https://example.com/developer",
        &excludes
    ));
}

#[test]
fn test_canonicalize_url_trailing_slash_and_fragment() {
    let a = canonicalize_url_for_dedupe("https://example.com/docs/");
    let b = canonicalize_url_for_dedupe("https://example.com/docs#intro");
    assert_eq!(a, b);
    assert_eq!(a.as_deref(), Some("https://example.com/docs"));
}

#[test]
fn test_canonicalize_url_root_and_default_port() {
    let a = canonicalize_url_for_dedupe("https://example.com:443/");
    let b = canonicalize_url_for_dedupe("https://example.com/");
    assert_eq!(a, b);
    assert_eq!(a.as_deref(), Some("https://example.com/"));
}

#[test]
fn test_map_seed_scope_uses_resolved_project_prefix() {
    let seed = "https://example.github.io/project";
    let resolved = "https://example.github.io/project/";

    let scope = derive_map_scope(seed, resolved).expect("scope");

    assert_eq!(scope.host, "example.github.io");
    assert_eq!(scope.path_prefix.as_deref(), Some("/project"));
}

#[test]
fn test_map_scope_allows_root_seed_without_path_filter() {
    let scope = derive_map_scope("https://example.github.io/", "https://example.github.io/")
        .expect("scope");

    assert_eq!(scope.path_prefix, None);
}

// Coverage checks are skipped when max_pages == 0 (uncapped), so this verifies
// the explicit single-page fallback rule rather than the coverage heuristic.
#[test]
fn test_fallback_uncapped_small_but_complete_site() {
    // 1-page site, healthy content, no thin pages, max_pages uncapped (0)
    assert!(should_fallback_to_chrome(
        &summary(1, 0, 1),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_uncapped_two_pages_still_retries_chrome() {
    // 2-page sites are also too small to trust as "complete" in HTTP mode.
    assert!(should_fallback_to_chrome(
        &summary(2, 0, 2),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_no_fallback_uncapped_three_pages_healthy() {
    // 3+ pages should use normal thin-ratio checks when uncapped.
    assert!(!should_fallback_to_chrome(
        &summary(3, 0, 3),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_no_fallback_uncapped_nine_pages_healthy() {
    // 9-page site, healthy, max_pages uncapped — should NOT trigger Chrome
    assert!(!should_fallback_to_chrome(
        &summary(9, 0, 9),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_uncapped_thin_ratio_still_fires() {
    // Even with uncapped, thin ratio > 60% should still trigger Chrome
    assert!(should_fallback_to_chrome(
        &summary(10, 7, 9),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_uncapped_zero_markdown_files() {
    // Zero markdown with max_pages=0 → still fall back
    assert!(should_fallback_to_chrome(
        &summary(5, 5, 0),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_custom_thin_ratio() {
    let mut cfg = default_cfg();
    // Lower the threshold so 50% thin triggers fallback
    cfg.auto_switch_thin_ratio = 0.40;
    // 50% thin: 5/10 = 0.50, above 0.40 → should fall back (max_pages=0 skips coverage check)
    assert!(should_fallback_to_chrome(&summary(10, 5, 8), 0, &cfg));
    // With default 0.60 threshold, 50% thin should NOT trigger thin-ratio fallback
    // Use max_pages=0 to bypass the coverage check and isolate thin-ratio behaviour.
    assert!(!should_fallback_to_chrome(
        &summary(10, 5, 8),
        0,
        &default_cfg()
    ));
}

#[test]
fn test_fallback_custom_min_pages() {
    let mut cfg = default_cfg();
    cfg.auto_switch_min_pages = 20;
    // max_pages=80: (80/10)=8, .max(20)=20 — coverage needs 20 files, we have 15 → fallback
    assert!(should_fallback_to_chrome(&summary(200, 0, 15), 80, &cfg));
    // With default min_pages=10: (80/10)=8, .max(10)=10 — 15 >= 10 → no coverage fallback
    assert!(!should_fallback_to_chrome(
        &summary(200, 0, 15),
        80,
        &default_cfg()
    ));
}

#[test]
fn test_map_retry_gate_low_coverage() {
    assert!(should_retry_map_with_chrome(&summary(0, 0, 0)));
    assert!(should_retry_map_with_chrome(&summary(1, 0, 1)));
    assert!(should_retry_map_with_chrome(&summary(2, 0, 2)));
    assert!(!should_retry_map_with_chrome(&summary(3, 0, 3)));
}

#[test]
fn test_should_retry_map_with_html_fallback_for_two_or_fewer_urls() {
    assert!(should_retry_map_with_html_fallback(0));
    assert!(should_retry_map_with_html_fallback(1));
    assert!(should_retry_map_with_html_fallback(2));
    assert!(!should_retry_map_with_html_fallback(3));
}

#[test]
fn test_merge_map_candidate_urls_adds_only_new_scoped_urls() {
    let scope = MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    };

    let merged = merge_map_candidate_urls(
        vec!["https://example.github.io/project/overview".to_string()],
        vec![
            "https://example.github.io/project/overview/".to_string(),
            "https://example.github.io/project/api".to_string(),
            "https://example.github.io/other/out-of-scope".to_string(),
        ],
        &scope,
        true,
    );

    assert_eq!(
        merged,
        vec![
            "https://example.github.io/project/overview".to_string(),
            "https://example.github.io/project/api".to_string(),
        ]
    );
}

#[test]
fn test_regex_escape_escapes_hyphen() {
    assert_eq!(regex_escape("foo-bar"), "foo\\-bar");
}

// --- Spider API wiring tests ---

#[test]
fn test_spider_retry_wiring_round_trips() {
    // Verify spider's with_retry() stores the value we pass from cfg.fetch_retries.
    // configure_website() calls with_retry(cfg.fetch_retries.min(u8::MAX) as u8)
    // when fetch_retries > 0; this test confirms the Spider API contract.
    let mut website = Website::new("https://example.com");
    website.with_retry(3);
    assert_eq!(website.configuration.retry, 3);
}

#[test]
fn test_spider_normalize_wiring_round_trips() {
    // Verify spider's with_normalize() stores the value we pass from cfg.normalize.
    let mut website = Website::new("https://example.com");
    website.with_normalize(true);
    assert!(website.configuration.normalize);
    website.with_normalize(false);
    assert!(!website.configuration.normalize);
}

#[test]
fn test_spider_tld_disabled_by_default() {
    // TLD crawling is hardcoded to false in configure_website(); verify the Spider
    // API default matches our expectation (i.e., with_tld(false) is a no-op baseline).
    let mut website = Website::new("https://example.com");
    website.with_tld(false);
    assert!(!website.configuration.tld);
}

#[test]
fn test_spider_block_assets_wiring() {
    let mut website = Website::new("https://example.com");
    website.with_block_assets(true);
    assert!(website.configuration.only_html);
}

#[test]
fn test_spider_redirect_policy_strict_wiring() {
    use spider::configuration::RedirectPolicy;
    let mut website = Website::new("https://example.com");
    website.with_redirect_policy(RedirectPolicy::Strict);
    assert_eq!(
        website.configuration.redirect_policy,
        RedirectPolicy::Strict
    );
}

#[test]
fn test_spider_whitelist_url_wiring() {
    let mut website = Website::new("https://example.com");
    let patterns: Vec<spider::compact_str::CompactString> =
        vec!["https://example.com/docs/.*".into()];
    website.with_whitelist_url(Some(patterns));
    assert!(website.configuration.whitelist_url.is_some());
}

#[test]
fn test_spider_max_page_bytes_wiring() {
    let mut website = Website::new("https://example.com");
    website.with_max_page_bytes(Some(1_048_576.0));
    assert_eq!(website.configuration.max_page_bytes, Some(1_048_576.0));
}

// --- Junk URL detection tests ---

#[test]
fn test_junk_url_html_encoded_ampersand() {
    // Extracted from HTML without entity decoding — always 404 on the server.
    assert!(is_junk_discovered_url(
        "https://github.com/trending/rust?since=daily&amp;spoken_language_code=en"
    ));
    assert!(is_junk_discovered_url(
        "https://example.com/search?q=foo&amp;page=2"
    ));
    // Real ampersand in query string is fine.
    assert!(!is_junk_discovered_url(
        "https://github.com/trending/rust?since=daily&spoken_language_code=en"
    ));
}

#[test]
fn test_junk_url_encoded_html_tags() {
    assert!(is_junk_discovered_url(
        "https://opencode.ai/introductionbelonging%20toclaimed%20thatconsequences%3Cmeta%20name="
    ));
    assert!(is_junk_discovered_url(
        "https://example.com/foo%3Cscript%3Ebar"
    ));
    assert!(is_junk_discovered_url("https://example.com/path%3ebar"));
}

#[test]
fn test_junk_url_template_literals() {
    assert!(is_junk_discovered_url(
        "https://opencode.ai/download/stable/$%7BshareBaseUrl%7D/s/$%7BshareId%7D"
    ));
    assert!(is_junk_discovered_url("https://example.com/%7Bfoo%7D"));
}

#[test]
fn test_junk_url_excessive_encoded_spaces() {
    // 3+ %20 = junk
    assert!(is_junk_discovered_url(
        "https://opencode.ai/an%20alternativeas%20a%20result%20ofpt"
    ));
    // 2 %20 = OK (e.g., /wiki/New%20York%20City)
    assert!(!is_junk_discovered_url(
        "https://en.wikipedia.org/wiki/New%20York%20City"
    ));
}

#[test]
fn test_junk_url_js_concat_artifacts() {
    assert!(is_junk_discovered_url(
        "https://opencode.ai/download/stable/'%20+%20e5%20+%20'"
    ));
    assert!(is_junk_discovered_url("https://example.com/foo'%20bar"));
    assert!(is_junk_discovered_url("https://example.com/foo%20'bar"));
}

#[test]
fn test_junk_url_too_long() {
    let long = format!("https://example.com/{}", "a".repeat(2048));
    assert!(is_junk_discovered_url(&long));
}

#[test]
fn test_junk_url_legit_urls_pass() {
    assert!(!is_junk_discovered_url(
        "https://example.com/docs/getting-started"
    ));
    assert!(!is_junk_discovered_url(
        "https://example.com/api/v1/users?q=hello%20world"
    ));
    assert!(!is_junk_discovered_url(
        "https://example.com/blog/my%20post"
    ));
    assert!(!is_junk_discovered_url("https://example.com/"));
    assert!(!is_junk_discovered_url("https://example.com"));
}

#[test]
fn test_junk_url_query_string_not_checked() {
    // %3C in query string should NOT trigger (only path is checked)
    assert!(!is_junk_discovered_url(
        "https://example.com/search?q=%3Cdiv%3E"
    ));
    // %7B in query string should NOT trigger
    assert!(!is_junk_discovered_url(
        "https://example.com/api?filter=%7Bname%7D"
    ));
}

#[test]
fn test_junk_url_relative_urls() {
    assert!(is_junk_discovered_url("/foo%3Cbar%3Ebaz"));
    assert!(is_junk_discovered_url("/$%7Bvar%7D/path"));
    assert!(!is_junk_discovered_url("/docs/getting-started"));
}

// --- Auto path-prefix scoping tests ---

#[test]
fn test_auto_whitelist_deep_path_is_scoped() {
    use super::url_utils::derive_auto_whitelist_pattern;
    let pattern = derive_auto_whitelist_pattern(
        "https://ai.google.dev/api/python/google/generativeai/GenerativeModel",
    );
    let pat = pattern.expect("expected Some pattern for deep path");
    assert!(
        pat.starts_with("^https?://ai\\.google\\.dev/api/python/google/generativeai"),
        "pattern should anchor to the directory prefix, got: {pat}"
    );
    assert!(
        pat.ends_with("(/|$)"),
        "pattern should end with (/|$), got: {pat}"
    );
}

#[test]
fn test_auto_whitelist_root_path_returns_none() {
    use super::url_utils::derive_auto_whitelist_pattern;
    let pattern = derive_auto_whitelist_pattern("https://example.com/");
    assert!(
        pattern.is_none(),
        "root path should produce no auto-scope, got: {pattern:?}"
    );
}

#[test]
fn test_auto_whitelist_root_no_slash_returns_none() {
    use super::url_utils::derive_auto_whitelist_pattern;
    let pattern = derive_auto_whitelist_pattern("https://example.com");
    assert!(
        pattern.is_none(),
        "empty path should produce no auto-scope, got: {pattern:?}"
    );
}

#[test]
fn test_auto_whitelist_single_segment_returns_none() {
    use super::url_utils::derive_auto_whitelist_pattern;
    // /docs is a single meaningful segment — broad enough, no scoping.
    let pattern = derive_auto_whitelist_pattern("https://example.com/docs");
    assert!(
        pattern.is_none(),
        "single-segment path should produce no auto-scope, got: {pattern:?}"
    );
}

#[test]
fn test_auto_whitelist_trailing_slash_handled() {
    use super::url_utils::derive_auto_whitelist_pattern;
    // Path ending with '/' — the directory IS the path itself.
    // /api/v1/ has two meaningful segments, so it should scope.
    let pattern = derive_auto_whitelist_pattern("https://example.com/api/v1/");
    let pat = pattern.expect("expected Some pattern for two-segment directory path");
    assert!(
        pat.contains("/api/v1"),
        "pattern should include /api/v1, got: {pat}"
    );
    assert!(
        pat.ends_with("(/|$)"),
        "pattern should end with (/|$), got: {pat}"
    );
}

#[test]
fn test_auto_whitelist_two_segment_path_scoped() {
    use super::url_utils::derive_auto_whitelist_pattern;
    // /api/v1/users — directory is /api/v1/, two segments → scoped.
    let pattern = derive_auto_whitelist_pattern("https://api.example.com/api/v1/users");
    let pat = pattern.expect("expected Some pattern for multi-segment path");
    assert!(
        pat.contains("/api/v1"),
        "pattern should scope to /api/v1, got: {pat}"
    );
}

#[test]
fn test_auto_whitelist_dots_in_host_escaped() {
    use super::url_utils::derive_auto_whitelist_pattern;
    let pattern =
        derive_auto_whitelist_pattern("https://docs.python.org/3/library/collections/OrderedDict");
    let pat = pattern.expect("expected Some pattern");
    assert!(
        pat.contains("docs\\.python\\.org"),
        "dots in host must be escaped, got: {pat}"
    );
}

// --- CDP hostname detection tests (Issue 1: explicit allowlist vs fragile heuristic) ---

#[test]
fn test_docker_service_host_only_rewrites_known_names() {
    use crate::crates::core::config::parse::is_docker_service_host;
    // Known Docker service names must be detected.
    assert!(is_docker_service_host("axon-chrome"));
    assert!(is_docker_service_host("axon-postgres"));
    // Hyphenated hosts NOT in the allowlist must NOT be rewritten.
    assert!(!is_docker_service_host("my-home-server"));
    assert!(!is_docker_service_host("custom-chrome-proxy"));
    // Plain hosts must not be rewritten.
    assert!(!is_docker_service_host("127.0.0.1"));
    assert!(!is_docker_service_host("localhost"));
}

use super::*;

fn excludes(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// 1. Empty excludes list → empty result.
#[test]
fn build_exclude_blacklist_patterns_returns_empty_for_no_excludes() {
    let patterns = build_exclude_blacklist_patterns("https://example.com", &[]);
    assert!(patterns.is_empty());
}

// 2. Pattern starts with `^https?://` and contains the escaped host.
#[test]
fn build_exclude_blacklist_patterns_generates_anchored_host_scoped_regex() {
    let patterns = build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr"]));
    assert_eq!(patterns.len(), 2);
    assert!(
        patterns[0].starts_with("^https?://"),
        "pattern should start with ^https?://, got: {}",
        patterns[0]
    );
    assert!(
        patterns[0].contains("example"),
        "pattern should contain host, got: {}",
        patterns[0]
    );
}

// 3. Dots in hostname are escaped to `\.`.
#[test]
fn build_exclude_blacklist_patterns_escapes_dots_in_hostname() {
    let patterns = build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr"]));
    assert_eq!(patterns.len(), 2);
    assert!(
        patterns[0].contains("example\\.com"),
        "dots in hostname should be escaped, got: {}",
        patterns[0]
    );
}

// 4. Prefix without leading slash and with leading slash produce the same pattern.
#[test]
fn build_exclude_blacklist_patterns_normalizes_prefix_without_leading_slash() {
    let with_slash = build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr"]));
    let without_slash = build_exclude_blacklist_patterns("https://example.com", &excludes(&["fr"]));
    assert_eq!(
        with_slash, without_slash,
        "prefix 'fr' and '/fr' should yield identical patterns"
    );
}

// 5. Three excludes → six patterns (root + first path segment relative).
#[test]
fn build_exclude_blacklist_patterns_multiple_excludes_produces_one_pattern_each() {
    let patterns =
        build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr", "/de", "/ja"]));
    assert_eq!(patterns.len(), 6);
}

// 6. Unparseable URL falls back to `[^/]+` as the host wildcard.
#[test]
fn build_exclude_blacklist_patterns_invalid_start_url_uses_wildcard_host() {
    let patterns = build_exclude_blacklist_patterns("not-a-valid-url", &excludes(&["/fr"]));
    assert_eq!(patterns.len(), 2);
    assert!(
        patterns[0].contains("[^/]+"),
        "invalid URL should fall back to [^/]+ host pattern, got: {}",
        patterns[0]
    );
}

// 7. Pattern ends with the boundary alternation group.
//    The format! uses `\\?` which produces `\?` in the output — a regex-escaped
//    literal question mark matching the start of a query string.
#[test]
fn build_exclude_blacklist_patterns_pattern_ends_with_boundary_alternation() {
    let patterns = build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr"]));
    assert_eq!(patterns.len(), 2);
    assert!(
        patterns[0].ends_with("(?:/|-|$|\\?|#)"),
        "pattern should end with boundary alternation group, got: {}",
        patterns[0]
    );
    assert!(
        patterns[1].contains("/[^/?#]+/fr"),
        "second pattern should match first path segment relative excludes, got: {}",
        patterns[1]
    );
}

#[test]
fn normalize_map_candidate_url_strips_fragment_and_trailing_slash() {
    let scope = MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    };

    let normalized = normalize_map_candidate_url(
        "https://example.github.io/project/docs/#intro",
        &scope,
        true,
    );

    assert_eq!(
        normalized.as_deref(),
        Some("https://example.github.io/project/docs")
    );
}

#[test]
fn normalize_map_candidate_url_rejects_out_of_scope_paths() {
    let scope = MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    };

    assert!(
        normalize_map_candidate_url("https://example.github.io/other/docs", &scope, true).is_none()
    );
}

#[test]
fn extract_link_host_returns_host_without_port() {
    assert_eq!(
        extract_link_host("https://github.com/foo"),
        Some("github.com")
    );
    assert_eq!(
        extract_link_host("https://github.com:443/foo"),
        Some("github.com")
    );
    assert_eq!(
        extract_link_host("http://127.0.0.1:8080/bar"),
        Some("127.0.0.1")
    );
}

#[test]
fn extract_link_host_returns_none_for_relative_urls() {
    assert_eq!(extract_link_host("/docs/guide"), None);
    assert_eq!(extract_link_host("relative/path"), None);
}

#[test]
fn extract_link_host_handles_ipv6() {
    assert_eq!(extract_link_host("http://[::1]:8080/"), Some("[::1]:8080"));
}

#[test]
fn normalize_map_candidate_url_drops_query_when_requested() {
    let scope = MapScope {
        host: "example.github.io".to_string(),
        path_prefix: Some("/project".to_string()),
    };

    assert_eq!(
        normalize_map_candidate_url("https://example.github.io/project/docs/?q=1", &scope, true)
            .as_deref(),
        Some("https://example.github.io/project/docs")
    );
}

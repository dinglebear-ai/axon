use spider::url::Url;
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapScope {
    pub(crate) host: String,
    pub(crate) path_prefix: Option<String>,
}

pub(crate) fn canonicalize_url_for_dedupe(url: &str) -> Option<String> {
    let mut parsed = Url::parse(url).ok()?;
    parsed.set_fragment(None);

    match (parsed.scheme(), parsed.port()) {
        ("http", Some(80)) | ("https", Some(443)) => {
            let _ = parsed.set_port(None);
        }
        _ => {}
    }

    let path = parsed.path().to_string();
    if path.len() > 1 {
        let normalized_path = path.trim_end_matches('/').to_string();
        parsed.set_path(&normalized_path);
    }

    Some(parsed.to_string())
}

pub(crate) fn normalize_map_candidate_url(
    raw: &str,
    scope: &MapScope,
    drop_query: bool,
) -> Option<String> {
    if is_junk_discovered_url(raw) {
        return None;
    }

    let mut parsed = Url::parse(raw).ok()?;
    parsed.set_fragment(None);

    if drop_query {
        parsed.set_query(None);
    }

    let host = parsed.host_str()?;
    if !host.eq_ignore_ascii_case(&scope.host) {
        return None;
    }

    if let Some(prefix) = scope.path_prefix.as_deref() {
        let path = parsed.path();
        if path != prefix
            && !path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('-'))
        {
            return None;
        }
    }

    canonicalize_url_for_dedupe(parsed.as_ref())
}

pub(crate) fn is_excluded_url_path(url: &str, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let path = Url::parse(url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_else(|| "/".to_string());
    is_excluded_path_or_first_segment_relative(&path, excludes)
}

pub(crate) fn is_excluded_path_or_first_segment_relative(path: &str, excludes: &[String]) -> bool {
    if excludes
        .iter()
        .any(|prefix| is_path_prefix_excluded(path, prefix))
    {
        return true;
    }

    let Some(rest) = path
        .trim_start_matches('/')
        .split_once('/')
        .map(|(_, rest)| format!("/{rest}"))
    else {
        return false;
    };
    excludes
        .iter()
        .any(|prefix| is_path_prefix_excluded(&rest, prefix))
}

fn is_path_prefix_excluded(path: &str, prefix: &str) -> bool {
    let normalized: Cow<'_, str> = if prefix.starts_with('/') {
        Cow::Borrowed(prefix)
    } else {
        Cow::Owned(format!("/{prefix}"))
    };
    let boundary = normalized.trim_end_matches('/');
    if boundary.is_empty() {
        return false;
    }
    path == boundary
        || path
            .strip_prefix(boundary)
            .is_some_and(|rest| rest.starts_with('/') || rest.starts_with('-'))
}

pub(crate) fn regex_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
            | '-' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Derive a whitelist regex pattern that scopes a crawl to the directory
/// subtree of `start_url`.
///
/// Returns `None` when the URL path is `/` or a single non-empty segment
/// (e.g. `/docs`), since those are already broad enough — no scoping needed.
///
/// Example: `https://ai.google.dev/api/python/google/generativeai/GenerativeModel`
/// → `^https?://ai\.google\.dev/api/python/google/generativeai(/|$)`
pub(crate) fn derive_auto_whitelist_pattern(
    start_url: &str,
) -> Option<spider::compact_str::CompactString> {
    let parsed = Url::parse(start_url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();

    // Find the directory prefix: everything up to and including the last `/`
    // before a filename, or the path itself when it ends with `/`.
    let dir_prefix = if path.ends_with('/') {
        path.to_string()
    } else {
        // rfind('/') always finds at least the leading '/' for absolute paths.
        let slash_pos = path.rfind('/')?;
        path[..=slash_pos].to_string()
    };

    // Count meaningful segments (non-empty parts after splitting on '/').
    let segment_count = dir_prefix.split('/').filter(|s| !s.is_empty()).count();

    // Root ("/") or single segment ("/docs/") — no auto-scoping.
    if segment_count <= 1 {
        return None;
    }

    // Strip trailing slash from the prefix for the regex (we add `(/|$)` ourselves).
    let prefix_for_regex = dir_prefix.trim_end_matches('/');
    let pattern = format!(
        "^https?://{}{}(/|$)",
        regex_escape(host),
        regex_escape(prefix_for_regex),
    );
    Some(spider::compact_str::CompactString::from(pattern))
}

pub(super) fn build_exclude_blacklist_patterns(
    start_url: &str,
    excludes: &[String],
) -> Vec<String> {
    let host_pattern = Url::parse(start_url)
        .ok()
        .and_then(|u| u.host_str().map(regex_escape))
        .unwrap_or_else(|| "[^/]+".to_string());

    excludes
        .iter()
        .flat_map(|prefix| {
            let normalized: Cow<'_, str> = if prefix.starts_with('/') {
                Cow::Borrowed(prefix)
            } else {
                Cow::Owned(format!("/{prefix}"))
            };
            let root_pattern = format!(
                "^https?://{}{}(?:/|-|$|\\?|#)",
                host_pattern,
                regex_escape(&normalized)
            );
            let first_segment_relative_pattern = format!(
                "^https?://{}/[^/?#]+{}(?:/|-|$|\\?|#)",
                host_pattern,
                regex_escape(&normalized)
            );
            [root_pattern, first_segment_relative_pattern]
        })
        .collect()
}

/// Extract the host portion from an absolute URL, stripping any port number.
///
/// Returns `None` for relative URLs (no `://`). Handles IPv6 bracket addresses
/// by leaving them intact when port-stripping would be ambiguous.
pub(super) fn extract_link_host(url: &str) -> Option<&str> {
    let i = url.find("://")?;
    let after = &url[i + 3..];
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let host_and_port = &after[..end];
    // Strip decimal port number when present; leave IPv6 bracket addresses intact.
    if !host_and_port.starts_with('[')
        && let Some(colon) = host_and_port.rfind(':')
    {
        let port_str = &host_and_port[colon + 1..];
        if !port_str.is_empty() && port_str.bytes().all(|b| b.is_ascii_digit()) {
            return Some(&host_and_port[..colon]);
        }
    }
    Some(host_and_port)
}

/// Returns `true` if the URL is garbage extracted from minified JS/CSS bundles
/// rather than a real hyperlink.
///
/// Spider's link extractor pulls anything that resembles a relative path from
/// page content — including `<script>` tags and inline JS. This produces URLs
/// like `https://example.com/belonging%20toclaimed%20that%3Cmeta%20name=` or
/// `https://example.com/$%7BshareBaseUrl%7D/s/$%7BshareId%7D`.
///
/// Heuristics (applied to the full URL, then path-only):
/// - URL length > 2048 (standard browser limit)
/// - HTML-encoded ampersand (`&amp;`) — URL came from unescaped HTML source and
///   was never decoded; these always 404 because the server expects `&`, not `&amp;`
/// - Encoded HTML tags: `%3C` (`<`) or `%3E` (`>`)
/// - Template literals: `%7B` (`{`) or `%7D` (`}`)
/// - 3+ encoded spaces (`%20`) — prose, not a URL
/// - JS concatenation artifacts: `'%20` or `%20'`
pub(crate) fn is_junk_discovered_url(url: &str) -> bool {
    if url.len() > 2048 {
        return true;
    }

    // HTML-encoded ampersand: appears when spider extracts links from raw HTML
    // without decoding entities. Real URLs never contain the literal string "&amp;".
    if url.contains("&amp;") {
        return true;
    }

    let path = url_path_portion(url);

    // Encoded HTML tags: < or > never appear in real URL paths.
    if path.contains("%3C") || path.contains("%3c") || path.contains("%3E") || path.contains("%3e")
    {
        return true;
    }

    // Template literal variables: { or } from JS `${variable}` expressions.
    if path.contains("%7B") || path.contains("%7b") || path.contains("%7D") || path.contains("%7d")
    {
        return true;
    }

    // 3+ encoded spaces in path = extracted prose, not a URL.
    // Real URLs use hyphens/underscores for word separation.
    if path.matches("%20").count() >= 3 {
        return true;
    }

    // JS string concatenation: `' + var + '` shows up as `'%20+%20var%20+%20'`.
    if path.contains("'%20") || path.contains("%20'") {
        return true;
    }

    false
}

/// Extract the path portion of a URL (between host and query/fragment).
/// For relative URLs (no scheme), treats the whole string up to `?` or `#` as path.
fn url_path_portion(url: &str) -> &str {
    let after_host = match url.find("://") {
        Some(i) => {
            let rest = &url[i + 3..];
            let path_start = rest.find('/').unwrap_or(rest.len());
            &rest[path_start..]
        }
        None => url,
    };
    let end = after_host
        .find('?')
        .or_else(|| after_host.find('#'))
        .unwrap_or(after_host.len());
    &after_host[..end]
}

#[cfg(test)]
#[path = "url_utils_proptest.rs"]
mod url_utils_proptest;

#[cfg(test)]
mod tests {
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
        let with_slash =
            build_exclude_blacklist_patterns("https://example.com", &excludes(&["/fr"]));
        let without_slash =
            build_exclude_blacklist_patterns("https://example.com", &excludes(&["fr"]));
        assert_eq!(
            with_slash, without_slash,
            "prefix 'fr' and '/fr' should yield identical patterns"
        );
    }

    // 5. Three excludes → six patterns (root + first path segment relative).
    #[test]
    fn build_exclude_blacklist_patterns_multiple_excludes_produces_one_pattern_each() {
        let patterns = build_exclude_blacklist_patterns(
            "https://example.com",
            &excludes(&["/fr", "/de", "/ja"]),
        );
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
            normalize_map_candidate_url("https://example.github.io/other/docs", &scope, true)
                .is_none()
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
            normalize_map_candidate_url(
                "https://example.github.io/project/docs/?q=1",
                &scope,
                true
            )
            .as_deref(),
            Some("https://example.github.io/project/docs")
        );
    }
}

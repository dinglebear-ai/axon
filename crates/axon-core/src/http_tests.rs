use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use crate::http::{
    cdp_discovery_url, normalize_url, ssrf_blacklist_patterns, validate_resolved_ips, validate_url,
};

// --- normalize_url tests ---

#[test]
fn normalize_url_adds_https_scheme_to_bare_host() {
    assert_eq!(normalize_url("example.com"), "https://example.com");
}

#[test]
fn normalize_url_adds_https_scheme_to_host_with_path() {
    assert_eq!(
        normalize_url("example.com/docs/install"),
        "https://example.com/docs/install"
    );
}

#[test]
fn normalize_url_preserves_existing_https_scheme() {
    assert_eq!(
        normalize_url("https://example.com/page"),
        "https://example.com/page"
    );
}

#[test]
fn normalize_url_preserves_existing_http_scheme() {
    assert_eq!(
        normalize_url("http://example.com/page"),
        "http://example.com/page"
    );
}

#[test]
fn normalize_url_preserves_path_and_query() {
    assert_eq!(
        normalize_url("example.com/path?key=value"),
        "https://example.com/path?key=value"
    );
}

#[test]
fn normalize_url_preserves_fragment() {
    assert_eq!(
        normalize_url("example.com/page#section"),
        "https://example.com/page#section"
    );
}

#[test]
fn normalize_url_trims_whitespace() {
    assert_eq!(normalize_url("  example.com  "), "https://example.com");
}

#[test]
fn normalize_url_returns_empty_for_empty_input() {
    assert_eq!(normalize_url(""), "");
}

#[test]
fn normalize_url_handles_localhost() {
    assert_eq!(normalize_url("localhost"), "https://localhost");
}

#[test]
fn normalize_url_handles_localhost_with_port() {
    // localhost:8080 contains a '.'-free host but starts with "localhost"
    assert_eq!(normalize_url("localhost:8080"), "https://localhost:8080");
}

#[test]
fn normalize_url_does_not_add_scheme_to_bare_text_with_spaces() {
    // A string with spaces is not a valid URL host — normalize_url leaves it as-is
    assert_eq!(normalize_url("not a url"), "not a url");
}

// --- Public URLs should be allowed ---

#[test]
fn validate_url_allows_public_https() {
    assert!(validate_url("https://example.com/").is_ok());
}

#[test]
fn validate_url_allows_public_http() {
    assert!(validate_url("http://example.com/page").is_ok());
}

// --- Loopback addresses ---

#[test]
fn validate_url_blocks_loopback_ipv4() {
    assert!(validate_url("http://127.0.0.1/").is_err());
}

#[test]
fn validate_url_blocks_localhost() {
    assert!(validate_url("http://localhost/").is_err());
}

#[test]
fn validate_url_blocks_ipv6_loopback() {
    assert!(validate_url("http://[::1]/").is_err());
}

// --- AWS metadata / link-local ---

#[test]
fn validate_url_blocks_aws_metadata() {
    assert!(validate_url("http://169.254.169.254/latest/meta-data/").is_err());
}

#[test]
fn validate_url_blocks_link_local_boundary() {
    // 169.254.169.253 is still in 169.254.0.0/16 — should be blocked
    assert!(validate_url("http://169.254.169.253/").is_err());
}

// --- RFC-1918 private ranges ---

#[test]
fn validate_url_blocks_10_network() {
    assert!(validate_url("http://10.0.0.1/").is_err());
}

#[test]
fn validate_url_blocks_10_network_upper() {
    assert!(validate_url("http://10.255.255.255/").is_err());
}

#[test]
fn validate_url_blocks_172_16() {
    assert!(validate_url("http://172.16.0.1/").is_err());
}

#[test]
fn validate_url_allows_172_15() {
    // 172.15.255.255 is just below the 172.16.0.0/12 range — should ALLOW
    assert!(validate_url("http://172.15.255.255/").is_ok());
}

#[test]
fn validate_url_allows_172_32() {
    // 172.32.0.0 is just above the 172.16-31 range — should ALLOW
    assert!(validate_url("http://172.32.0.0/").is_ok());
}

#[test]
fn validate_url_blocks_192_168() {
    assert!(validate_url("http://192.168.0.1/").is_err());
}

// --- Blocked URL schemes ---

#[test]
fn validate_url_blocks_ftp() {
    assert!(validate_url("ftp://example.com/").is_err());
}

#[test]
fn validate_url_blocks_file() {
    assert!(validate_url("file:///etc/passwd").is_err());
}

#[test]
fn validate_url_blocks_data() {
    assert!(validate_url("data:text/plain,hello").is_err());
}

// --- TLD blocking ---

#[test]
fn validate_url_blocks_internal_tld() {
    assert!(validate_url("http://host.internal/").is_err());
}

#[test]
fn validate_url_blocks_local_tld() {
    assert!(validate_url("http://host.local/").is_err());
}

#[test]
fn validate_url_blocks_internal_tld_case_insensitive() {
    assert!(validate_url("http://HOST.INTERNAL/").is_err());
}

// --- Invalid URLs ---

#[test]
fn validate_url_blocks_invalid_url() {
    assert!(validate_url("not a valid url at all").is_err());
}

// --- IPv6 private ranges ---

#[test]
fn validate_url_blocks_ipv6_ula() {
    // fc00::1 is unique-local address (fc00::/7)
    assert!(validate_url("http://[fc00::1]/").is_err());
}

#[test]
fn validate_url_blocks_ipv6_link_local() {
    // fe80::1 is link-local (fe80::/10)
    assert!(validate_url("http://[fe80::1]/").is_err());
}

/// Compiled SSRF blacklist regexes — built once, reused across tests.
static COMPILED_SSRF_PATTERNS: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    ssrf_blacklist_patterns()
        .iter()
        .map(|p| regex::Regex::new(p).expect("ssrf blacklist pattern must compile"))
        .collect()
});

#[test]
fn ssrf_blacklist_blocks_localhost_with_query() {
    let url = "http://localhost?admin=true";
    let blocked = COMPILED_SSRF_PATTERNS.iter().any(|re| re.is_match(url));
    assert!(
        blocked,
        "localhost with query string should be blocked by blacklist"
    );
}

// --- cdp_discovery_url tests ---

#[test]
fn cdp_discovery_url_http_appends_json_version() {
    assert_eq!(
        cdp_discovery_url("http://127.0.0.1:6000"),
        Some("http://127.0.0.1:6000/json/version".to_string())
    );
}

#[test]
fn cdp_discovery_url_ws_converts_to_http_and_appends() {
    assert_eq!(
        cdp_discovery_url("ws://axon-chrome:9222"),
        Some("http://axon-chrome:9222/json/version".to_string())
    );
}

#[test]
fn cdp_discovery_url_preserves_non_root_path() {
    // Already has /json/version — must not double-append.
    assert_eq!(
        cdp_discovery_url("http://127.0.0.1:6000/json/version"),
        Some("http://127.0.0.1:6000/json/version".to_string())
    );
}

#[test]
fn cdp_discovery_url_rejects_unsupported_scheme() {
    assert_eq!(cdp_discovery_url("ftp://host:21/"), None);
    assert_eq!(cdp_discovery_url("file:///etc/hosts"), None);
}

#[test]
fn cdp_discovery_url_wss_converts_to_https() {
    assert_eq!(
        cdp_discovery_url("wss://secure-host:443"),
        Some("https://secure-host:443/json/version".to_string())
    );
}

#[test]
fn cdp_discovery_url_ws_with_existing_path_preserved() {
    // Pre-resolved ws:// URL with browser UUID path: path must not be clobbered.
    let ws = "ws://127.0.0.1:9222/devtools/browser/abc-123";
    let result = cdp_discovery_url(ws);
    assert_eq!(
        result,
        Some("http://127.0.0.1:9222/devtools/browser/abc-123".to_string())
    );
}

#[test]
fn ssrf_blacklist_blocks_localhost_with_fragment() {
    let url = "https://localhost#secret";
    let blocked = COMPILED_SSRF_PATTERNS.iter().any(|re| re.is_match(url));
    assert!(
        blocked,
        "localhost with fragment should be blocked by blacklist"
    );
}

/// Documents that `validate_url()` performs parse-time checks while
/// `SsrfBlockingResolver` handles the connect-time TOCTOU window.
///
/// Both layers are required: `validate_url()` blocks literal IPs and hostile TLDs
/// immediately (no DNS roundtrip needed); `SsrfBlockingResolver` catches any
/// hostname that resolves to a private IP at the moment reqwest dials.
#[test]
fn dns_rebinding_toctou_is_mitigated_by_resolver() {
    // A public hostname passes parse-time validation. In production builds,
    // SsrfBlockingResolver then validates the resolved IP at connect time.
    assert!(
        validate_url("https://attacker-controlled.example.com/").is_ok(),
        "public hostname should pass parse-time check"
    );
    // Direct private IPs are caught at parse time, before any DNS lookup.
    assert!(
        validate_url("http://127.0.0.1/").is_err(),
        "direct loopback must be blocked at parse time"
    );
    assert!(
        validate_url("http://[::1]/").is_err(),
        "direct loopback IPv6 must be blocked at parse time"
    );
}

#[test]
fn resolved_hostname_ips_reject_private_addresses() {
    let err = validate_resolved_ips(
        "attacker.example",
        [
            IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        ],
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("127.0.0.1"),
        "expected loopback address in error: {err}"
    );
}

#[test]
fn resolved_hostname_ips_reject_ipv6_unique_local_addresses() {
    let err = validate_resolved_ips(
        "attacker.example",
        [IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1))],
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("fd00"),
        "expected unique-local address in error: {err}"
    );
}

/// Verifies that public IPs pass and private IPs fail parse-time validation.
#[test]
fn validate_url_accepts_public_ip_rejects_private() {
    assert!(
        validate_url("http://93.184.216.34/").is_ok(),
        "public IP should pass validation"
    );
    assert!(validate_url("http://10.0.0.1/").is_err());
    assert!(validate_url("http://192.168.1.1/").is_err());
}

/// Verifies the LazyLock SSRF pattern compilation works and all patterns are valid.
#[test]
fn ssrf_blacklist_patterns_compile_once() {
    let patterns = &*COMPILED_SSRF_PATTERNS;
    assert!(
        !patterns.is_empty(),
        "SSRF blacklist should have at least one pattern"
    );
    assert_eq!(
        patterns.len(),
        ssrf_blacklist_patterns().len(),
        "compiled pattern count must match raw pattern count"
    );
    assert!(
        patterns
            .iter()
            .any(|re| re.is_match("http://127.0.0.1/admin")),
        "loopback URL should match at least one SSRF pattern"
    );
}

// --- IPv4-mapped IPv6 bypass tests ---

#[test]
fn validate_url_rejects_ipv4_mapped_ipv6_loopback() {
    assert!(
        validate_url("http://[::ffff:127.0.0.1]/").is_err(),
        "::ffff:127.0.0.1 must be blocked as loopback"
    );
}

#[test]
fn validate_url_rejects_ipv4_mapped_ipv6_link_local() {
    assert!(
        validate_url("http://[::ffff:169.254.0.1]/").is_err(),
        "::ffff:169.254.0.1 must be blocked as link-local"
    );
}

#[test]
fn validate_url_rejects_ipv4_mapped_ipv6_private() {
    assert!(
        validate_url("http://[::ffff:10.0.0.1]/").is_err(),
        "::ffff:10.0.0.1 must be blocked as private"
    );
    assert!(
        validate_url("http://[::ffff:192.168.1.1]/").is_err(),
        "::ffff:192.168.1.1 must be blocked as private"
    );
    assert!(
        validate_url("http://[::ffff:172.16.0.1]/").is_err(),
        "::ffff:172.16.0.1 must be blocked as private"
    );
}

#[test]
fn validate_url_allows_ipv4_mapped_ipv6_public() {
    assert!(
        validate_url("http://[::ffff:93.184.216.34]/").is_ok(),
        "::ffff: with public IPv4 should be allowed"
    );
}

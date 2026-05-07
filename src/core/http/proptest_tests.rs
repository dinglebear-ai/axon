//! Property-based tests for `validate_url` (SSRF guard).
//!
//! These complement the hand-written cases in `tests.rs` by generating
//! adversarial inputs across full IP ranges — catching bypass patterns that
//! manual test cases miss.

use crate::core::http::validate_url;
use proptest::prelude::*;

// ── Private IPv4 ranges ──────────────────────────────────────────────────────

proptest! {
    /// Every address in 10.0.0.0/8 must be rejected, regardless of path/port.
    #[test]
    fn validate_url_rejects_all_10_network(
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://10.{b}.{c}.{d}/");
        prop_assert!(
            validate_url(&url).is_err(),
            "10.x.x.x must be rejected as private RFC-1918: {url}"
        );
    }
}

proptest! {
    /// Every address in 172.16.0.0/12 (172.16–172.31) must be rejected.
    #[test]
    fn validate_url_rejects_all_172_16_to_31(
        b in 16u8..=31,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://172.{b}.{c}.{d}/path");
        prop_assert!(
            validate_url(&url).is_err(),
            "172.16–31.x.x must be rejected as private RFC-1918: {url}"
        );
    }
}

proptest! {
    /// Addresses just outside the 172.16.0.0/12 range must be allowed.
    /// This guards the boundary logic against off-by-one errors.
    #[test]
    fn validate_url_allows_172_outside_private_range(
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        // 172.15.x.x and 172.32.x.x are public — must not be blocked.
        let below = format!("http://172.15.{c}.{d}/");
        let above = format!("http://172.32.{c}.{d}/");
        prop_assert!(
            validate_url(&below).is_ok(),
            "172.15.x.x is public and must be allowed: {below}"
        );
        prop_assert!(
            validate_url(&above).is_ok(),
            "172.32.x.x is public and must be allowed: {above}"
        );
    }
}

proptest! {
    /// Every address in 192.168.0.0/16 must be rejected.
    #[test]
    fn validate_url_rejects_all_192_168(
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://192.168.{c}.{d}/");
        prop_assert!(
            validate_url(&url).is_err(),
            "192.168.x.x must be rejected as private RFC-1918: {url}"
        );
    }
}

proptest! {
    /// Every address in 127.0.0.0/8 (loopback) must be rejected.
    #[test]
    fn validate_url_rejects_all_127_loopback(
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://127.{b}.{c}.{d}/");
        prop_assert!(
            validate_url(&url).is_err(),
            "127.x.x.x loopback must be rejected: {url}"
        );
    }
}

proptest! {
    /// Link-local 169.254.0.0/16 (AWS metadata endpoint range) must be rejected.
    #[test]
    fn validate_url_rejects_all_link_local(
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://169.254.{c}.{d}/latest/meta-data/");
        prop_assert!(
            validate_url(&url).is_err(),
            "169.254.x.x link-local must be rejected: {url}"
        );
    }
}

// ── Non-HTTP/HTTPS schemes ───────────────────────────────────────────────────

proptest! {
    /// Any URL that uses a scheme other than http/https must be rejected.
    /// Generates scheme names that are guaranteed to be non-HTTP.
    #[test]
    fn validate_url_rejects_non_http_schemes(
        suffix in "[a-z]{2,8}",
    ) {
        // Skip http and https — they are the only allowed schemes.
        prop_assume!(suffix != "http" && suffix != "https");
        let url = format!("{suffix}://example.com/path");
        // The URL may fail to parse entirely (invalid scheme) or be rejected by
        // the scheme check.  Either outcome is an error — never Ok.
        prop_assert!(
            validate_url(&url).is_err(),
            "non-HTTP/S scheme must be rejected: {url}"
        );
    }
}

// ── Public IPs must not panic ────────────────────────────────────────────────

proptest! {
    /// Arbitrary byte-quartet URLs (all four octets in 0–255) must never panic.
    /// Whether they pass or fail validation is not asserted — only no-panic.
    #[test]
    fn validate_url_never_panics_on_any_ipv4(
        a in 0u8..=255,
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://{a}.{b}.{c}.{d}/");
        // Ignore the result — we only care that this does not panic.
        let _ = validate_url(&url);
    }
}

proptest! {
    /// Arbitrary alphanumeric hostnames with random paths must never panic.
    #[test]
    fn validate_url_never_panics_on_random_hostnames(
        host in "[a-z0-9]{1,20}(\\.[a-z]{2,6}){1,3}",
        path in "(/[a-z0-9_-]{0,20}){0,5}",
    ) {
        let url = format!("https://{host}{path}");
        let _ = validate_url(&url);
    }
}

// ── IPv4-mapped IPv6 bypass surface ─────────────────────────────────────────

proptest! {
    /// Every ::ffff: address embedding a private IPv4 must be rejected.
    /// This guards the IPv4-mapped bypass path that was found in the original
    /// security review.
    #[test]
    fn validate_url_rejects_ipv4_mapped_private_10(
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://[::ffff:10.{b}.{c}.{d}]/");
        prop_assert!(
            validate_url(&url).is_err(),
            "::ffff:10.x.x.x must be rejected as private: {url}"
        );
    }
}

proptest! {
    /// Every ::ffff: address embedding a 192.168.x.x private IPv4 must be rejected.
    #[test]
    fn validate_url_rejects_ipv4_mapped_private_192_168(
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://[::ffff:192.168.{c}.{d}]/");
        prop_assert!(
            validate_url(&url).is_err(),
            "::ffff:192.168.x.x must be rejected as private: {url}"
        );
    }
}

proptest! {
    /// Every ::ffff: address embedding a 127.x.x.x loopback must be rejected.
    #[test]
    fn validate_url_rejects_ipv4_mapped_loopback(
        b in 0u8..=255,
        c in 0u8..=255,
        d in 0u8..=255,
    ) {
        let url = format!("http://[::ffff:127.{b}.{c}.{d}]/");
        prop_assert!(
            validate_url(&url).is_err(),
            "::ffff:127.x.x.x must be rejected as loopback: {url}"
        );
    }
}

//! SSRF protection: URL validation and IP range blocking.

use spider::url::Url;
use std::net::IpAddr;

use super::error::HttpError;
use super::normalize::normalize_url;

// Test-only thread-local flag: when set, `validate_url` permits loopback
// addresses so httpmock servers on 127.0.0.1 can be reached by code under
// test. Production builds never see this — the flag is `#[cfg(test)]`-gated.
//
// Thread-local avoids cross-thread races with SSRF tests that assert
// loopback is blocked. Code that spawns tokio tasks (e.g. JoinSet) must
// propagate the flag via `get_allow_loopback()` + `set_allow_loopback()`
// in the spawned task.
#[cfg(test)]
thread_local! {
    static ALLOW_LOOPBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Set the thread-local loopback bypass flag. Only available in test builds.
#[cfg(test)]
pub(crate) fn set_allow_loopback(allow: bool) {
    ALLOW_LOOPBACK.with(|c| c.set(allow));
}

/// Read the thread-local loopback bypass flag. Only available in test builds.
/// Used by code that spawns tasks to propagate the flag to child threads.
#[cfg(test)]
pub(crate) fn get_allow_loopback() -> bool {
    ALLOW_LOOPBACK.with(|c| c.get())
}

/// Reject URLs that would allow SSRF attacks.
///
/// Blocks:
/// - Non-http/https schemes
/// - Loopback addresses (127.0.0.0/8, ::1)
/// - Link-local addresses (169.254.0.0/16, fe80::/10)
/// - RFC-1918 private ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
/// - `.internal` and `.local` TLDs
///
/// # Errors
///
/// Returns `Err` if the URL is malformed, uses a non-HTTP(S) scheme, or resolves
/// to a blocked address range.
///
/// # DNS Rebinding (TOCTOU residual risk)
///
/// This validation is TOCTOU — it checks the resolved IP at parse time, but
/// `reqwest` resolves DNS independently at connect time. An attacker with a
/// TTL-0 DNS record can pass validation (first resolution → public IP) then
/// rebind before the connection is established (second resolution → 127.0.0.1).
///
/// Full mitigation requires DNS pre-resolution and connection pinning, which
/// `reqwest` does not support natively. Consider adding a reverse-DNS check or
/// using `hickory-resolver` for pre-resolution if the threat model includes
/// attacker-controlled domains with short-TTL records.
///
/// As defence-in-depth, `ssrf_blacklist_patterns()` is also applied to
/// discovered URLs during crawl via spider's `with_blacklist_url()`.
pub fn validate_url(url: &str) -> Result<(), HttpError> {
    let normalized = normalize_url(url);
    let parsed = Url::parse(&normalized).map_err(|_| HttpError::InvalidUrl(url.to_string()))?;

    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(HttpError::BlockedScheme(s.to_string())),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| HttpError::InvalidUrl(url.to_string()))?;

    // Block localhost and .internal/.local TLDs
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Err(HttpError::BlockedHost(host.to_string()));
    }
    if lower.ends_with(".internal") || lower.ends_with(".local") {
        return Err(HttpError::BlockedHost(host.to_string()));
    }

    // Use host_str() + parse::<IpAddr>() directly. Do NOT use
    // spider::url::Host::Ipv4/Ipv6 enum variants — they silently fail for IPv6
    // (confirmed production bug, see CLAUDE.md).
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        check_ip(ip)?;
    }

    Ok(())
}

/// SSRF IP validation — checks loopback, link-local, RFC-1918 private, and
/// IPv4-mapped IPv6 addresses. Extracted as a named function (not a closure)
/// so the IPv4-mapped branch can recurse into the IPv4 checks.
fn check_ip(ip: IpAddr) -> Result<(), HttpError> {
    #[cfg(test)]
    {
        if ip.is_loopback() && ALLOW_LOOPBACK.with(|c| c.get()) {
            return Ok(());
        }
    }
    if ip.is_loopback() || ip.is_unspecified() {
        return Err(HttpError::BlockedIpRange(ip));
    }
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            let octets = v4.octets();
            let is_link_local = octets[0] == 169 && octets[1] == 254;
            let is_private = octets[0] == 10
                || (a == 172 && (16..=31).contains(&b))
                || octets[0..2] == [192, 168];
            if is_link_local || is_private {
                return Err(HttpError::BlockedIpRange(IpAddr::V4(v4)));
            }
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 (::ffff:x.x.x.x) — extract the embedded IPv4
            // and apply the same private/loopback/link-local checks. Without this,
            // ::ffff:127.0.0.1 bypasses the V4 branch entirely.
            if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                return check_ip(IpAddr::V4(mapped_v4));
            }

            // Block unique-local (fc00::/7) and link-local (fe80::/10)
            let segs = v6.segments();
            let is_unique_local = segs[0] & 0xfe00 == 0xfc00;
            let is_link_local_v6 = segs[0] & 0xffc0 == 0xfe80;
            if is_unique_local || is_link_local_v6 {
                return Err(HttpError::BlockedIpRange(IpAddr::V6(v6)));
            }
        }
    }
    Ok(())
}

/// SSRF defence-in-depth patterns for spider.rs `with_blacklist_url()`.
///
/// Covers RFC-1918 private ranges, loopback, link-local, and IPv6 private addresses.
/// Use alongside `validate_url()` on the seed URL so discovered URLs are also blocked.
pub(crate) fn ssrf_blacklist_patterns() -> &'static [&'static str] {
    &[
        r"^https?://127\.",
        r"^https?://10\.",
        r"^https?://192\.168\.",
        r"^https?://172\.(1[6-9]|2[0-9]|3[01])\.",
        r"^https?://169\.254\.",
        r"^https?://0\.",
        r"^https?://localhost([^a-zA-Z0-9]|$)",
        r"^https?://\[::1\]",
        r"^https?://\[::ffff:",
        r"^https?://\[fe80:",
        r"^https?://\[fc[0-9a-f]{2}:",
        r"^https?://\[fd[0-9a-f]{2}:",
    ]
}

/// SSRF blacklist patterns pre-converted to `CompactString` for spider's
/// `with_blacklist_url()`. Avoids the 5x duplication of the
/// `.iter().copied().map(Into::into).collect()` chain across callers.
pub(crate) fn ssrf_blacklist_compact_strings() -> Vec<spider::compact_str::CompactString> {
    ssrf_blacklist_patterns()
        .iter()
        .copied()
        .map(Into::into)
        .collect()
}

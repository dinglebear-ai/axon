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
/// Returns `Err` if the URL is malformed, uses a non-HTTP(S) scheme, uses a
/// blocked host, or contains a literal blocked IP address.
///
/// # DNS Rebinding (TOCTOU — MITIGATED)
///
/// This function performs hostname-level checks only (blocked names, TLDs, literal IPs).
/// The DNS rebinding TOCTOU window is closed by [`SsrfBlockingResolver`], which is wired
/// into the reqwest HTTP client via `ClientBuilder::dns_resolver()`. The resolver re-runs
/// `check_ip()` on every IP that the OS returns, at the moment the TCP connection is
/// established — the same instant reqwest would connect. This means even a TTL-0 DNS
/// record that flips to `127.0.0.1` after `validate_url()` is caught at connection time.
///
/// Test builds skip the custom resolver (see [`SsrfBlockingResolver`]) so that httpmock
/// servers on `127.0.0.1` remain reachable. `validate_url()` still blocks loopback
/// in tests unless `set_allow_loopback(true)` is called.
///
/// As defence-in-depth, `ssrf_blacklist_patterns()` is also applied to
/// discovered URLs during crawl via spider's `with_blacklist_url()`.
pub fn validate_url(url: &str) -> Result<(), HttpError> {
    let parsed = parse_http_url(url)?;
    validate_host(url, parsed.host_str())?;
    Ok(())
}

fn parse_http_url(url: &str) -> Result<Url, HttpError> {
    let normalized = normalize_url(url);
    let parsed = Url::parse(&normalized).map_err(|_| HttpError::InvalidUrl(url.to_string()))?;

    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(HttpError::BlockedScheme(s.to_string())),
    }

    Ok(parsed)
}

fn validate_host(url: &str, host: Option<&str>) -> Result<(), HttpError> {
    let host = host.ok_or_else(|| HttpError::InvalidUrl(url.to_string()))?;

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

/// Validate a URL and reject hostnames that resolve to Axon's blocked SSRF IP
/// ranges.
///
/// This is used before handing URLs to Spider, whose HTTP stack does not use
/// Axon's reqwest `SsrfBlockingResolver`.
pub async fn validate_url_with_dns(url: &str) -> Result<(), HttpError> {
    let parsed = parse_http_url(url)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| HttpError::InvalidUrl(url.to_string()))?;
    validate_host(url, Some(host))?;

    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    if bare_host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| HttpError::DnsResolution {
            host: host.to_string(),
            error: error.to_string(),
        })?;
    validate_resolved_ips(host, addrs.map(|addr| addr.ip()))
}

pub(crate) fn validate_resolved_ips(
    host: &str,
    ips: impl IntoIterator<Item = IpAddr>,
) -> Result<(), HttpError> {
    for ip in ips {
        if check_ip(ip).is_err() {
            return Err(HttpError::BlockedResolvedIp {
                host: host.to_string(),
                ip,
            });
        }
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
            let is_link_local = a == 169 && b == 254;
            let is_private =
                a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
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

/// A DNS resolver that validates each resolved IP against the SSRF blocklist.
///
/// Plugged into the reqwest HTTP client via `ClientBuilder::dns_resolver()` to
/// close the DNS rebinding TOCTOU window: [`check_ip`] runs at actual TCP connection
/// time, on the same IPs that reqwest will dial. This eliminates the gap between
/// [`validate_url`]'s parse-time check and reqwest's connect-time DNS resolution.
///
/// Passed directly (without `Arc::new`) — reqwest 0.13's `dns_resolver()` accepts
/// any `R: Resolve + 'static` via the `IntoResolve` blanket impl, so wrapping in
/// `Arc` is no longer required.
///
/// Only compiled for non-test builds. Tests use reqwest's default resolver so
/// httpmock servers on `127.0.0.1` remain reachable.
#[cfg(not(test))]
pub(crate) struct SsrfBlockingResolver;

#[cfg(not(test))]
impl reqwest::dns::Resolve for SsrfBlockingResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            type DnsError = Box<dyn std::error::Error + Send + Sync>;

            let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{host}:0"))
                .await
                .map_err(|e| Box::new(e) as DnsError)?
                .collect();

            let allowed: Vec<std::net::SocketAddr> = addrs
                .into_iter()
                .filter(|addr| check_ip(addr.ip()).is_ok())
                .collect();

            if allowed.is_empty() {
                let err: DnsError = Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF: all resolved IPs for '{host}' are in blocked ranges"),
                ));
                return Err(err);
            }

            Ok(Box::new(allowed.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// SSRF blacklist patterns pre-converted to `CompactString` for spider's
/// `with_blacklist_url()`. Computed once via `LazyLock` to avoid repeated
/// allocation on every call. Callers that need a `Vec` for spider's API
/// can `.to_vec()` the returned slice.
pub(crate) fn ssrf_blacklist_compact_strings() -> &'static [spider::compact_str::CompactString] {
    use std::sync::LazyLock;
    static PATTERNS: LazyLock<Vec<spider::compact_str::CompactString>> = LazyLock::new(|| {
        ssrf_blacklist_patterns()
            .iter()
            .copied()
            .map(Into::into)
            .collect()
    });
    &PATTERNS
}

//! Structured [`SecurityAuditEvent`] production for SSRF checks.
//!
//! Split out of `ssrf.rs` (monolith policy: ≤500 lines/file) but stays a
//! child module of `ssrf` so it can reuse `ssrf`'s private helpers
//! (`parse_http_url`, `check_ip`) without widening their visibility.
//!
//! Per `docs/pipeline-unification/runtime/security-contract.md` ("SSRF
//! Policy"): every fetched URL records requested URL, canonical URL,
//! resolved IP class, redirect chain position, policy decision, and a
//! redacted-headers indicator. This module builds that record; `axon-core`
//! does not depend on `axon-observe`, so it never owns sink delivery —
//! callers with a sink (jobs/services layer) convert via
//! `axon_observe::security_audit::emit_security_audit`. The one exception is
//! the connect-time resolver denial path below, which has no caller to hand
//! a record to, so it self-reports via `tracing`.

use std::net::IpAddr;

use axon_api::source::{
    ResolvedIpClass, SecurityAuditEvent, SecurityAuditEventKind, SecurityPolicyDecision,
    SsrfAuditDetail,
};

use super::super::error::HttpError;
use super::super::normalize::normalize_url;

/// Validate a URL and return a structured [`SecurityAuditEvent`] alongside
/// the pass/fail result, per the security contract's "SSRF Policy" section:
/// every fetched URL records requested URL, canonical URL, resolved IP
/// class, redirect chain position, policy decision, and a redacted-headers
/// indicator.
///
/// `redirect_chain_index` is `0` for the original request URL and increments
/// for each hop a caller resolves manually (this crate does not itself
/// follow redirects here). `headers_present` should be `true` when the
/// caller is about to attach custom headers to this fetch — the audit
/// record never carries header *values*, only this boolean indicator.
///
/// This does not perform DNS resolution; for a literal-IP URL,
/// `resolved_ip_class` reflects that IP. For a hostname URL, use
/// [`validate_resolved_ips_with_audit`] once resolution has happened (e.g.
/// via [`super::validate_url_with_dns`]) to get a real IP class.
pub fn validate_url_with_audit(
    url: &str,
    redirect_chain_index: u32,
    headers_present: bool,
) -> (Result<(), HttpError>, SecurityAuditEvent) {
    let canonical_url = normalize_url(url);
    let resolved_ip_class = literal_host_ip_class(&canonical_url);
    let result = super::validate_url(url);
    build_ssrf_audit(
        url,
        &canonical_url,
        resolved_ip_class,
        redirect_chain_index,
        headers_present,
        &result,
    )
}

/// Same as [`super::validate_resolved_ips`], but also returns a structured
/// [`SecurityAuditEvent`] carrying the actual resolved IP class (as opposed
/// to [`validate_url_with_audit`], which cannot see DNS results for hostname
/// URLs).
pub fn validate_resolved_ips_with_audit(
    url: &str,
    host: &str,
    ips: impl IntoIterator<Item = IpAddr>,
    redirect_chain_index: u32,
    headers_present: bool,
) -> (Result<(), HttpError>, SecurityAuditEvent) {
    let canonical_url = normalize_url(url);
    let ips: Vec<IpAddr> = ips.into_iter().collect();
    let result = super::validate_resolved_ips(host, ips.iter().copied());
    // Report the class of the first blocked IP on denial (matches the IP the
    // error carries); otherwise the first resolved IP, defaulting to
    // `NotResolved` if the resolver returned nothing.
    let resolved_ip_class = match &result {
        Err(HttpError::BlockedResolvedIp { ip, .. }) => classify_ip(*ip),
        _ => ips
            .first()
            .copied()
            .map(classify_ip)
            .unwrap_or(ResolvedIpClass::NotResolved),
    };
    build_ssrf_audit(
        url,
        &canonical_url,
        resolved_ip_class,
        redirect_chain_index,
        headers_present,
        &result,
    )
}

/// Emit an audit record for the connect-time [`super::SsrfBlockingResolver`]
/// denial path via `tracing`. This is the primary SSRF enforcement point for
/// every fetch made through the shared reqwest client, but it only sees a
/// hostname (not the original request URL or headers) — `requested_url` and
/// `canonical_url` both fall back to the bare host, and `headers_redacted`
/// is unknown at this layer so it is reported `true` (fail closed: assume
/// headers may have been present rather than under-reporting).
// Only called from `SsrfBlockingResolver::resolve`, which is itself
// `#[cfg(not(test))]` (tests use reqwest's default resolver so httpmock
// servers on 127.0.0.1 stay reachable) — legitimately unused under `cfg(test)`.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn record_resolver_denial(host: &str, blocked_ips: Vec<IpAddr>) {
    let resolved_ip_class = blocked_ips
        .first()
        .copied()
        .map(classify_ip)
        .unwrap_or(ResolvedIpClass::NotResolved);
    let detail = SsrfAuditDetail {
        requested_url: host.to_string(),
        canonical_url: host.to_string(),
        resolved_ip_class,
        redirect_chain_index: 0,
        policy_decision: SecurityPolicyDecision::Deny,
        headers_redacted: true,
    };
    let event = SecurityAuditEvent::new(
        SecurityAuditEventKind::SsrfDenied,
        format!("all resolved IPs for '{host}' are in blocked ranges"),
    )
    .with_policy("axon-core-ssrf-resolver", env!("CARGO_PKG_VERSION"))
    .with_ssrf_detail(detail);

    tracing::warn!(
        event_id = %event.event_id,
        kind = "ssrf_denied",
        host = %host,
        resolved_ip_class = ?resolved_ip_class,
        blocked_ip_count = blocked_ips.len(),
        reason = %event.reason,
        "security_audit.ssrf_denied"
    );
}

fn build_ssrf_audit(
    requested_url: &str,
    canonical_url: &str,
    resolved_ip_class: ResolvedIpClass,
    redirect_chain_index: u32,
    headers_present: bool,
    result: &Result<(), HttpError>,
) -> (Result<(), HttpError>, SecurityAuditEvent) {
    let policy_decision = if result.is_ok() {
        SecurityPolicyDecision::Allow
    } else {
        SecurityPolicyDecision::Deny
    };
    let reason = match result {
        Ok(()) => "ssrf policy check passed".to_string(),
        Err(err) => redact_ssrf_reason(err),
    };
    let detail = SsrfAuditDetail {
        requested_url: requested_url.to_string(),
        canonical_url: canonical_url.to_string(),
        resolved_ip_class,
        redirect_chain_index,
        policy_decision,
        headers_redacted: headers_present,
    };
    let event = SecurityAuditEvent::new(SecurityAuditEventKind::SsrfDenied, reason)
        .with_policy("axon-core-ssrf", env!("CARGO_PKG_VERSION"))
        .with_ssrf_detail(detail);
    let cloned_result = match result {
        Ok(()) => Ok(()),
        Err(err) => Err(clone_http_error(err)),
    };
    (cloned_result, event)
}

/// `HttpError` does not implement `Clone` (it wraps `reqwest::Error` via
/// `#[from]`), so audit builders that need to both return the original
/// `Result` and describe it in a `reason` string reconstruct a
/// same-shape error rather than consuming the original.
fn clone_http_error(err: &HttpError) -> HttpError {
    match err {
        HttpError::InvalidUrl(url) => HttpError::InvalidUrl(url.clone()),
        HttpError::BlockedScheme(scheme) => HttpError::BlockedScheme(scheme.clone()),
        HttpError::BlockedHost(host) => HttpError::BlockedHost(host.clone()),
        HttpError::BlockedIpRange(ip) => HttpError::BlockedIpRange(*ip),
        HttpError::BlockedResolvedIp { host, ip } => HttpError::BlockedResolvedIp {
            host: host.clone(),
            ip: *ip,
        },
        HttpError::DnsResolution { host, error } => HttpError::DnsResolution {
            host: host.clone(),
            error: error.clone(),
        },
        // Network errors are not expected from validate_url/validate_resolved_ips
        // (neither performs a network request), but cover the variant so this
        // stays exhaustive if that ever changes.
        HttpError::Network(_) => HttpError::InvalidUrl(String::new()),
    }
}

/// Redact an [`HttpError`] into an audit-safe reason string. Never echoes the
/// raw requested URL (which may carry userinfo credentials or tokens in the
/// query string) — only structural fields (scheme, host, IP) that the
/// contract explicitly allows in audit records.
fn redact_ssrf_reason(err: &HttpError) -> String {
    match err {
        HttpError::InvalidUrl(_) => "invalid or unparsable url".to_string(),
        HttpError::BlockedScheme(scheme) => format!("blocked scheme '{scheme}'"),
        HttpError::BlockedHost(host) => format!("blocked host '{host}'"),
        HttpError::BlockedIpRange(ip) => format!("blocked ip range '{ip}'"),
        HttpError::BlockedResolvedIp { host, ip } => {
            format!("host '{host}' resolved to blocked ip '{ip}'")
        }
        HttpError::DnsResolution { host, .. } => format!("dns resolution failed for '{host}'"),
        HttpError::Network(_) => "network error during ssrf validation".to_string(),
    }
}

/// Classify a literal IP address embedded in `url`'s host, without DNS
/// resolution. Returns [`ResolvedIpClass::NotResolved`] for hostname URLs
/// (no literal IP present) or unparsable URLs.
fn literal_host_ip_class(url: &str) -> ResolvedIpClass {
    let Ok(parsed) = super::parse_http_url(url) else {
        return ResolvedIpClass::NotResolved;
    };
    let Some(host) = parsed.host_str() else {
        return ResolvedIpClass::NotResolved;
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(ip) => classify_ip(ip),
        Err(_) => ResolvedIpClass::NotResolved,
    }
}

/// Classify a resolved IP address into the [`ResolvedIpClass`] audit
/// taxonomy. Mirrors `ssrf::check_ip`'s blocked-range logic but returns a
/// classification instead of a pass/fail result.
fn classify_ip(ip: IpAddr) -> ResolvedIpClass {
    if ip.is_loopback() {
        return ResolvedIpClass::Loopback;
    }
    if ip.is_unspecified() {
        return ResolvedIpClass::Unspecified;
    }
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            let is_link_local = a == 169 && b == 254;
            let is_private =
                a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
            if is_link_local {
                ResolvedIpClass::LinkLocal
            } else if is_private {
                ResolvedIpClass::Private
            } else {
                ResolvedIpClass::Public
            }
        }
        IpAddr::V6(v6) => {
            if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                return classify_ip(IpAddr::V4(mapped_v4));
            }
            let segs = v6.segments();
            let is_unique_local = segs[0] & 0xfe00 == 0xfc00;
            let is_link_local_v6 = segs[0] & 0xffc0 == 0xfe80;
            if is_unique_local {
                ResolvedIpClass::UniqueLocal
            } else if is_link_local_v6 {
                ResolvedIpClass::LinkLocal
            } else {
                ResolvedIpClass::Public
            }
        }
    }
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;

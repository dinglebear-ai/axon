//! Typed HTTP validation errors for SSRF protection and URL validation.

use std::net::IpAddr;

use thiserror::Error;

/// Typed HTTP validation errors for SSRF protection and URL validation.
#[derive(Debug, Error)]
pub enum HttpError {
    /// URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    /// URL uses a non-http/https scheme (e.g. ftp://, file://).
    #[error("blocked URL scheme '{0}': only http/https allowed")]
    BlockedScheme(String),
    /// Hostname is blocked (localhost, .internal, .local).
    #[error("blocked host '{0}'")]
    BlockedHost(String),
    /// IP address falls in a blocked range (loopback, link-local, RFC-1918).
    #[error("blocked IP '{0}': private/reserved range")]
    BlockedIpRange(IpAddr),
    /// Hostname resolves to a blocked address.
    #[error("blocked host '{host}': resolved to blocked IP '{ip}'")]
    BlockedResolvedIp { host: String, ip: IpAddr },
    /// Hostname could not be resolved.
    #[error("DNS resolution failed for '{host}': {error}")]
    DnsResolution { host: String, error: String },
    /// Network-level error from reqwest.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

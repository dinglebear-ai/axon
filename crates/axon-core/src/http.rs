//! HTTP client and URL validation utilities.
//!
//! [`http_client()`] returns a shared [`reqwest::Client`] backed by a [`LazyLock`].
//! [`validate_url()`] enforces SSRF protection: private IP ranges, loopback, and
//! metadata endpoints are rejected. HTTP clients also use a blocking DNS resolver
//! for connect-time SSRF checks; use [`validate_url_with_dns()`] before handing
//! URLs to non-reqwest fetchers.

mod acquire;
pub(crate) mod antibot;
mod cdp;
pub(crate) mod client;
mod conditional;
pub(crate) mod error;
mod headers;
#[cfg(feature = "tls-fingerprinting")]
pub(crate) mod impersonate;
pub(crate) mod normalize;
#[cfg(test)]
mod proptest_tests;
pub(crate) mod ssrf;
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
mod ua;
mod url_path;

// Re-export the full public API so downstream `use crate::http::*` continues to work.
pub use acquire::{
    DEFAULT_CHALLENGE_SCAN_BYTES, FetchError, FetchWebOptions, WebDocKind, WebDocument, fetch_web,
    fetch_web_html, is_block_like_status,
};
pub use antibot::{ChallengeDetection, detect_challenge};
pub use client::build_client_no_redirect;
pub use client::build_ssrf_guarded_client_builder;
pub use client::internal_service_http_client;
pub use client::{build_client, fetch_html, http_client};
pub use conditional::{Probe, conditional_probe};
pub use error::HttpError;
pub use headers::{parse_custom_headers, validate_custom_header_policy};
#[cfg(feature = "tls-fingerprinting")]
pub use impersonate::{fetch_html_impersonated, impersonating_client};
pub use normalize::normalize_url;
#[cfg(any(test, feature = "test-util"))]
pub use ssrf::LoopbackGuard;
pub use ssrf::validate_resolved_ips;
#[cfg(any(test, feature = "test-util"))]
pub use ssrf::{get_allow_loopback, set_allow_loopback};
pub use ssrf::{ssrf_blacklist_compact_strings, ssrf_blacklist_patterns};
pub use ssrf::{validate_resolved_ips_with_audit, validate_url_with_audit};
pub use ssrf::{validate_url, validate_url_with_dns};
pub use ua::{AXON_API_UA, DEFAULT_UA, axon_api_ua, axon_ua};
pub use url_path::with_path;

pub use cdp::cdp_discovery_url;

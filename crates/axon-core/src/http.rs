//! HTTP client and URL validation utilities.
//!
//! [`http_client()`] returns a shared [`reqwest::Client`] backed by a [`LazyLock`].
//! [`validate_url()`] enforces SSRF protection: private IP ranges, loopback, and
//! metadata endpoints are rejected. HTTP clients also use a blocking DNS resolver
//! for connect-time SSRF checks; use [`validate_url_with_dns()`] before handing
//! URLs to non-reqwest fetchers.

mod antibot;
mod cdp;
mod client;
mod conditional;
mod error;
mod headers;
mod normalize;
#[cfg(test)]
mod proptest_tests;
mod ssrf;
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
mod ua;
mod url_path;

// Re-export the full public API so downstream `use crate::http::*` continues to work.
pub use antibot::{ChallengeDetection, detect_challenge};
pub use client::build_client_no_redirect;
pub use client::build_ssrf_guarded_client_builder;
pub use client::internal_service_http_client;
pub use client::{build_client, fetch_html, http_client};
pub use conditional::{Probe, conditional_probe};
pub use error::HttpError;
pub use headers::{parse_custom_headers, validate_custom_header_policy};
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

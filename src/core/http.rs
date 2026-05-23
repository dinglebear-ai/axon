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

// Re-export the full public API so downstream `use crate::core::http::*` continues to work.
pub use antibot::{ChallengeDetection, detect_challenge};
pub(crate) use client::internal_service_http_client;
pub use client::{build_client, fetch_html, http_client};
pub(crate) use client::{build_client_no_redirect, build_client_without_ssrf_resolver};
pub use error::HttpError;
pub use headers::parse_custom_headers;
pub use normalize::normalize_url;
#[cfg(test)]
pub(crate) use ssrf::validate_resolved_ips;
#[cfg(test)]
pub(crate) use ssrf::{get_allow_loopback, set_allow_loopback};
pub(crate) use ssrf::{ssrf_blacklist_compact_strings, ssrf_blacklist_patterns};
pub use ssrf::{validate_url, validate_url_with_dns};
pub use ua::{AXON_API_UA, DEFAULT_UA, axon_api_ua, axon_ua};

pub(crate) use cdp::cdp_discovery_url;

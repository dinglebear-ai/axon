//! HTTP client and URL validation utilities.
//!
//! [`http_client()`] returns a shared [`reqwest::Client`] backed by a [`LazyLock`].
//! [`validate_url()`] enforces SSRF protection: private IP ranges, loopback, and
//! metadata endpoints are rejected. Note that this is a best-effort check — DNS
//! rebinding can still bypass it at request time (TOCTOU).

mod cdp;
mod client;
mod error;
mod headers;
mod normalize;
#[cfg(test)]
mod proptest_tests;
mod ssrf;
#[cfg(test)]
mod tests;

// Re-export the full public API so downstream `use crate::core::http::*` continues to work.
pub(crate) use client::internal_service_http_client;
pub use client::{build_client, fetch_html, http_client};
pub use error::HttpError;
pub use headers::parse_custom_headers;
pub use normalize::normalize_url;
pub use ssrf::validate_url;
#[cfg(test)]
pub(crate) use ssrf::{get_allow_loopback, set_allow_loopback};
pub(crate) use ssrf::{ssrf_blacklist_compact_strings, ssrf_blacklist_patterns};

pub(crate) use cdp::cdp_discovery_url;

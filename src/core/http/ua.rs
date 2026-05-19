//! User-Agent resolution for all HTTP calls.
//!
//! Two UA flavours:
//!
//! - **Browser UA** ([`axon_ua()`] / [`DEFAULT_UA`]): looks like a real Firefox
//!   browser. Used by the HTTP client singleton, Spider crawl/scrape paths, and
//!   verticals that scrape HTML pages (Amazon, eBay, YouTube). No axon identifier —
//!   bot-detection filters on many retail sites check for unknown tokens.
//!
//! - **API UA** ([`axon_api_ua()`] / [`AXON_API_UA`]): identifies the bot. Used by
//!   verticals that call structured JSON APIs (crates.io, npm, PyPI, GitHub, etc.).
//!   These APIs either require bot identification (crates.io policy) or rate-limit
//!   by UA — having a stable, identifiable UA there is correct behaviour.
//!
//! Both respect `AXON_USER_AGENT` as a global override when set.

use std::sync::LazyLock;

/// Default browser User-Agent — plain Firefox on Linux, no bot identifiers.
/// Used by the HTTP client singleton and all web-scraping paths so bot-detection
/// filters on retail/content sites don't trigger on unknown tokens.
pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

/// Bot-identifying User-Agent for structured API calls.
/// Used by verticals that talk to package registry APIs (crates.io, npm, PyPI,
/// GitHub, Docker Hub, etc.) where identifying the requester is required or
/// preferred. These services are bot-friendly and use the UA for rate-limit
/// attribution rather than blocking.
pub const AXON_API_UA: &str = concat!(
    "axon/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/jmagar/axon_rust)"
);

static RESOLVED_UA: LazyLock<String> = LazyLock::new(|| {
    std::env::var("AXON_USER_AGENT")
        .or_else(|_| std::env::var("AXON_CHROME_USER_AGENT"))
        .unwrap_or_else(|_| DEFAULT_UA.to_string())
});

static RESOLVED_API_UA: LazyLock<String> = LazyLock::new(|| {
    // Honour AXON_USER_AGENT as a global override; otherwise use the bot UA.
    std::env::var("AXON_USER_AGENT").unwrap_or_else(|_| AXON_API_UA.to_string())
});

/// Browser UA for web scraping — clean Firefox string, no bot tokens.
///
/// Use for: HTTP client singleton, Spider crawl/scrape paths, HTML-scraping
/// verticals (Amazon, eBay, YouTube). Override via `AXON_USER_AGENT`.
pub fn axon_ua() -> &'static str {
    RESOLVED_UA.as_str()
}

/// Bot-identifying UA for structured API calls.
///
/// Use for: package registry APIs (crates.io, npm, PyPI, GitHub, Docker Hub,
/// HuggingFace, dev.to, Shopify product JSON). Override via `AXON_USER_AGENT`.
pub fn axon_api_ua() -> &'static str {
    RESOLVED_API_UA.as_str()
}

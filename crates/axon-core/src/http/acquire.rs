//! The single sanctioned entry point for fetching web content for acquisition.
//!
//! # Why this module exists
//!
//! Before this, every acquisition surface built its own client and made its own
//! decisions: `scrape` checked only the status code and had no bot-wall
//! handling, `map` had a TLS-impersonation retry that nothing else could reach,
//! and the sitemap / llms.txt / backfill fetches built clients with **no
//! User-Agent at all**. The result was that a fix applied to one surface did
//! not reach the others — `axon map` recovered four Akamai-fronted sites while
//! `axon scrape` on the same host still captured a 380-byte "Access Denied"
//! page and dropped it as thin content, reporting success.
//!
//! Everything that pulls bytes off the public web for acquisition goes through
//! [`fetch_web`]. Adding a capability here reaches every surface at once, and
//! `cargo xtask check-fetch-divergence` fails the build if a new acquisition
//! client is constructed outside it.
//!
//! # The escalation ladder
//!
//! 1. Fetch with the shared SSRF-guarded client (browser UA, 10-hop redirect
//!    cap, connect-time DNS guard).
//! 2. Classify the response. A block-like status ([`is_block_like_status`]) or
//!    a body matching a WAF fingerprint ([`detect_challenge`]) is a *wall*, not
//!    content — distinct from an ordinary 404 or timeout.
//! 3. On a wall, and only on a wall, retry through the browser TLS/HTTP2
//!    impersonating client (feature `tls-fingerprinting`).
//! 4. Re-classify. A wall that survives escalation is returned as
//!    [`FetchError::Challenge`] so callers can surface it, rather than letting
//!    a denial page flow downstream as if it were a thin page.

use crate::http::antibot::{ChallengeDetection, detect_challenge};
use crate::http::client::http_client;
use crate::http::error::HttpError;
use crate::http::normalize::normalize_url;
use crate::http::ssrf::validate_url;

/// Default body budget for challenge fingerprint scanning.
///
/// Mirrors the antibot module's documented default so callers that have no
/// `Config` in scope still classify identically to those that do.
pub const DEFAULT_CHALLENGE_SCAN_BYTES: usize = 150 * 1024;

/// What kind of document a caller expects, which decides whether the body is
/// worth scanning for a bot-challenge fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebDocKind {
    /// An HTML page. Challenge fingerprints are HTML, so these are scanned.
    Html,
    /// A non-HTML support document (robots.txt, sitemap.xml, llms.txt).
    ///
    /// Still escalated on a block-like *status*, but the body is not
    /// fingerprint-scanned: these formats have their own structural validation
    /// (a sitemap must have a `<urlset>` root, llms.txt a leading heading) and
    /// scanning them would only add false positives.
    Support,
}

/// Tuning for a single [`fetch_web`] call.
#[derive(Debug, Clone)]
pub struct FetchWebOptions {
    pub kind: WebDocKind,
    /// Body budget for challenge scanning. Callers holding a `Config` should
    /// pass `cfg.antibot_max_body_scan_bytes`.
    pub challenge_scan_bytes: usize,
    /// Allow escalation to the impersonating client. Off for callers that must
    /// not pay the extra request (e.g. liveness probes).
    pub allow_escalation: bool,
}

impl Default for FetchWebOptions {
    fn default() -> Self {
        Self {
            kind: WebDocKind::Html,
            challenge_scan_bytes: DEFAULT_CHALLENGE_SCAN_BYTES,
            allow_escalation: true,
        }
    }
}

impl FetchWebOptions {
    pub fn html() -> Self {
        Self::default()
    }

    pub fn support() -> Self {
        Self {
            kind: WebDocKind::Support,
            ..Self::default()
        }
    }

    pub fn with_scan_bytes(mut self, bytes: usize) -> Self {
        self.challenge_scan_bytes = bytes;
        self
    }

    pub fn without_escalation(mut self) -> Self {
        self.allow_escalation = false;
        self
    }
}

/// A successful acquisition fetch.
#[derive(Debug, Clone)]
pub struct WebDocument {
    pub body: String,
    pub status: u16,
    /// URL after redirects.
    pub final_url: String,
    /// Whether the impersonating client was needed to get this body. Surfaced
    /// so callers can log/measure how often the plain client is walled off.
    pub escalated: bool,
}

/// Why an acquisition fetch failed.
#[derive(Debug)]
pub enum FetchError {
    /// A bot wall that survived every available escalation. Distinct from
    /// `Http` so callers never mistake a denial page for thin content.
    Challenge {
        url: String,
        status: u16,
        detection: Option<ChallengeDetection>,
    },
    /// Ordinary transport/validation failure.
    Http(HttpError),
    /// Non-success status that is not a bot wall (404, 500, …).
    Status { url: String, status: u16 },
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Challenge {
                url,
                status,
                detection,
            } => {
                let vendor = detection
                    .as_ref()
                    .map(|d| d.vendor.as_str())
                    .unwrap_or("unknown");
                write!(
                    f,
                    "bot challenge from {vendor} blocked {url} (HTTP {status})"
                )
            }
            Self::Http(e) => write!(f, "{e}"),
            Self::Status { url, status } => write!(f, "{url} returned HTTP {status}"),
        }
    }
}

impl std::error::Error for FetchError {}

impl From<HttpError> for FetchError {
    fn from(e: HttpError) -> Self {
        Self::Http(e)
    }
}

/// Statuses that commonly front a bot wall rather than a genuine absence.
///
/// Deliberately narrow: escalating on 404 or 500 would fire a second
/// (expensive, BoringSSL) request for every dead link in a crawl.
pub fn is_block_like_status(status: u16) -> bool {
    matches!(status, 401 | 403 | 406 | 429 | 503)
}

/// Fetch a web document through the shared acquisition ladder.
pub async fn fetch_web(url: &str, opts: &FetchWebOptions) -> Result<WebDocument, FetchError> {
    let normalized = normalize_url(url);
    validate_url(&normalized)?;
    let client = http_client().map_err(|e| {
        FetchError::Http(HttpError::DnsResolution {
            host: normalized.to_string(),
            error: e.to_string(),
        })
    })?;

    let response = client
        .get(normalized.as_ref())
        .send()
        .await
        .map_err(HttpError::from)?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let body = response.text().await.map_err(HttpError::from)?;

    let detection = classify(&body, status, opts);
    let walled = is_block_like_status(status) || detection.is_some();

    if !walled {
        return finish(body, status, final_url, false, url);
    }

    match escalate(url, opts).await {
        Some(Ok(doc)) => {
            let redetected = classify(&doc.body, doc.status, opts);
            if is_block_like_status(doc.status) || redetected.is_some() {
                return Err(FetchError::Challenge {
                    url: url.to_string(),
                    status: doc.status,
                    detection: redetected,
                });
            }
            Ok(doc)
        }
        // Escalation unavailable or itself failed: report the original wall.
        _ => Err(FetchError::Challenge {
            url: url.to_string(),
            status,
            detection,
        }),
    }
}

/// Convenience wrapper for callers that only want the body of an HTML page.
pub async fn fetch_web_html(url: &str) -> Result<String, FetchError> {
    fetch_web(url, &FetchWebOptions::html())
        .await
        .map(|d| d.body)
}

fn classify(body: &str, status: u16, opts: &FetchWebOptions) -> Option<ChallengeDetection> {
    if opts.kind != WebDocKind::Html {
        return None;
    }
    let _ = status;
    detect_challenge(body, |_| None, opts.challenge_scan_bytes)
}

fn finish(
    body: String,
    status: u16,
    final_url: String,
    escalated: bool,
    url: &str,
) -> Result<WebDocument, FetchError> {
    if !(200..300).contains(&status) {
        return Err(FetchError::Status {
            url: url.to_string(),
            status,
        });
    }
    Ok(WebDocument {
        body,
        status,
        final_url,
        escalated,
    })
}

#[cfg(feature = "tls-fingerprinting")]
async fn escalate(url: &str, opts: &FetchWebOptions) -> Option<Result<WebDocument, FetchError>> {
    if !opts.allow_escalation {
        return None;
    }
    match crate::http::impersonate::fetch_html_impersonated(url).await {
        Ok(body) => Some(Ok(WebDocument {
            body,
            status: 200,
            final_url: url.to_string(),
            escalated: true,
        })),
        Err(_) => None,
    }
}

#[cfg(not(feature = "tls-fingerprinting"))]
async fn escalate(_url: &str, _opts: &FetchWebOptions) -> Option<Result<WebDocument, FetchError>> {
    None
}

#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;

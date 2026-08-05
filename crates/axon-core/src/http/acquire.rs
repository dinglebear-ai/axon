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
//!    impersonating client, which is compiled into every supported Axon build.
//! 4. Re-classify. A wall that survives escalation is returned as
//!    [`FetchError::Challenge`] so callers can surface it, rather than letting
//!    a denial page flow downstream as if it were a thin page.

use crate::http::antibot::{ChallengeDetection, detect_challenge};
use crate::http::client::http_client;
use crate::http::error::HttpError;
use crate::http::normalize::normalize_url;
use crate::http::ssrf::validate_url;
use crate::logging::log_warn;

/// Default body budget for challenge fingerprint scanning.
///
/// Mirrors the antibot module's documented default so callers that have no
/// `Config` in scope still classify identically to those that do.
pub const DEFAULT_CHALLENGE_SCAN_BYTES: usize = 150 * 1024;

/// Tuning for a single [`fetch_web`] call.
#[derive(Debug, Clone)]
pub struct FetchWebOptions {
    /// Body budget for challenge scanning. Callers holding a `Config` should
    /// pass `cfg.antibot_max_body_scan_bytes`.
    pub challenge_scan_bytes: usize,
    /// Allow escalation to the impersonating client.
    pub allow_escalation: bool,
}

impl Default for FetchWebOptions {
    fn default() -> Self {
        Self {
            challenge_scan_bytes: DEFAULT_CHALLENGE_SCAN_BYTES,
            allow_escalation: true,
        }
    }
}

impl FetchWebOptions {
    pub fn html() -> Self {
        Self::default()
    }

    pub fn with_scan_bytes(mut self, bytes: usize) -> Self {
        self.challenge_scan_bytes = bytes;
        self
    }
}

/// A successful acquisition fetch.
#[derive(Debug, Clone)]
pub struct WebDocument {
    pub body: String,
    pub status: u16,
    /// URL after redirects.
    ///
    /// Load-bearing for provenance, not a convenience: on the escalated path a
    /// redirect can move the request to a different origin, and attributing
    /// those bytes to the original URL is exactly what would hide an SSRF.
    pub final_url: String,
    /// Whether the impersonating client was needed to get this body.
    pub escalated: bool,
}

/// What happened when a wall was hit, so an operator can tell a permanent block
/// from a transient failure or a missing build feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationOutcome {
    /// Impersonation ran and the wall was still there. This is a real block.
    StillWalled,
    /// Impersonation ran and broke (DNS, TLS, timeout, SSRF block). The site is
    /// NOT known to be permanently walled — retrying is reasonable.
    ClientInitializationFailed(String),
    /// The client initialized, but the impersonated request failed.
    RequestFailed(String),
    /// The caller explicitly disabled escalation by configuration.
    Disabled,
}

impl std::fmt::Display for EscalationOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StillWalled => write!(f, "wall survived browser TLS impersonation"),
            Self::ClientInitializationFailed(reason) => {
                write!(f, "impersonating client initialization failed: {reason}")
            }
            Self::RequestFailed(reason) => write!(f, "impersonated request failed: {reason}"),
            Self::Disabled => write!(f, "impersonation disabled by configuration"),
        }
    }
}

/// Why an acquisition fetch failed.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// A bot wall. `escalation` records what the ladder was able to do about
    /// it, so a transient escalation fault is never reported as a permanent
    /// block — that misdiagnosis makes an operator abandon a working domain.
    #[error("bot wall blocked {url} (HTTP {status}, vendor {}): {escalation}", describe_vendor(.detection))]
    Challenge {
        url: String,
        status: u16,
        detection: Option<ChallengeDetection>,
        escalation: EscalationOutcome,
    },
    /// Ordinary transport/validation failure.
    #[error(transparent)]
    Http(#[from] HttpError),
    /// Non-success status that is not a bot wall (404, 500, ...).
    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },
}

fn describe_vendor(detection: &Option<ChallengeDetection>) -> &'static str {
    detection
        .as_ref()
        .map(|d| d.vendor.as_str())
        .unwrap_or("unknown")
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

    let detection = classify(&body, opts);
    let walled = is_block_like_status(status) || detection.is_some();

    if !walled {
        return finish(body, status, final_url, false, url);
    }

    match escalate(url, opts).await {
        Escalation::Fetched(doc) => {
            let redetected = classify(&doc.body, opts);
            if is_block_like_status(doc.status) || redetected.is_some() {
                return Err(FetchError::Challenge {
                    url: doc.final_url,
                    status: doc.status,
                    detection: redetected,
                    escalation: EscalationOutcome::StillWalled,
                });
            }
            finish(doc.body, doc.status, doc.final_url, true, url)
        }
        // Escalation ran and broke: do NOT present that as a permanent wall.
        // An operator told "bot challenge" gives up on the domain; told
        // "escalation failed: dns timeout" they retry.
        Escalation::ClientInitializationFailed(reason) => Err(FetchError::Challenge {
            url: url.to_string(),
            status,
            detection,
            escalation: EscalationOutcome::ClientInitializationFailed(reason),
        }),
        Escalation::RequestFailed(reason) => Err(FetchError::Challenge {
            url: url.to_string(),
            status,
            detection,
            escalation: EscalationOutcome::RequestFailed(reason),
        }),
        Escalation::Disabled => Err(FetchError::Challenge {
            url: url.to_string(),
            status,
            detection,
            escalation: EscalationOutcome::Disabled,
        }),
    }
}

/// Convenience wrapper for callers that only want the body of an HTML page.
pub async fn fetch_web_html(url: &str) -> Result<String, FetchError> {
    fetch_web(url, &FetchWebOptions::html())
        .await
        .map(|d| d.body)
}

/// Fingerprint-scan a body for a WAF challenge.
///
/// Deliberately status-independent: Cloudflare and DataDome serve challenge
/// pages with HTTP 200, so gating this on status would miss them entirely.
fn classify(body: &str, opts: &FetchWebOptions) -> Option<ChallengeDetection> {
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

/// Outcome of attempting the impersonated retry.
enum Escalation {
    /// The impersonating client returned a response.
    Fetched(WebDocument),
    /// Escalation ran and failed for an infrastructure reason (DNS, TLS,
    /// timeout, SSRF block). Distinct from a surviving wall so a transient
    /// network fault is never reported to an operator as a permanent block.
    ClientInitializationFailed(String),
    /// The client initialized but the request or response body failed.
    RequestFailed(String),
    /// The caller disabled escalation.
    Disabled,
}

async fn escalate(url: &str, opts: &FetchWebOptions) -> Escalation {
    if !opts.allow_escalation {
        return Escalation::Disabled;
    }
    match crate::http::impersonate::fetch_html_impersonated(url).await {
        Ok(resp) => Escalation::Fetched(WebDocument {
            body: resp.body,
            status: resp.status,
            final_url: resp.final_url,
            escalated: true,
        }),
        Err(HttpError::ImpersonationInit(reason)) => {
            log_warn(&format!(
                "acquire: impersonating client initialization failed for {url}: {reason}"
            ));
            Escalation::ClientInitializationFailed(reason)
        }
        Err(e) => {
            log_warn(&format!(
                "acquire: impersonated request failed for {url}: {e}"
            ));
            Escalation::RequestFailed(e.to_string())
        }
    }
}

#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;

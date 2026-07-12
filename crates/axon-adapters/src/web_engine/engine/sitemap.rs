//! Sitemap discovery + post-crawl backfill.
//!
//! Module root holds the shared bounded-HTTP fetch helpers used by both
//! halves (and by llms.txt discovery); the submodules own the pipeline:
//! - [`discover`] — robots.txt/seed-path sitemap discovery
//! - [`backfill`] — fetch missed URLs, convert, append to manifest
//! - [`filter`] — URL scope, `<lastmod>`, and markdown-passthrough checks

mod backfill;
mod discover;
mod filter;

pub use backfill::append_candidate_backfill;
pub use backfill::{BackfillStats, append_sitemap_backfill};
pub use discover::{SitemapDiscovery, discover_sitemap_urls};
#[cfg(test)]
use filter::is_already_markdown;
pub use filter::loc_in_scope;

use axon_core::config::Config;
use axon_core::http::validate_url;
use axon_core::logging::log_warn;
use spider::url::Url;
use std::error::Error;
use std::time::Duration;

/// spider 2.51 synthesises precise transport failures into the 52x range:
/// 521 connection refused, 522/523/524 timeouts, 525 DNS/NXDOMAIN, and
/// 526 host/TLS unreachable. `is_server_error()` is `true` for all of them,
/// so the naive "retry every 5xx" rule burns the retry budget re-resolving
/// dead hosts. 525 (NXDOMAIN) and 526 (host/TLS unreachable) are permanent
/// failures — retrying cannot help — so they are excluded. The remaining
/// 52x codes (refused/timeout) and genuine upstream 5xx stay retryable.
/// Bead axon_rust-6i30.
fn is_permanent_dead_host_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 525 | 526)
}

/// Default body cap for the `/llms.txt` discovery document (and small docs like robots.txt).
/// Guards the discovery path — NOT general HTML/sitemap fetches — against OOM from a
/// malicious/misconfigured host. 512 KB comfortably exceeds a real llms.txt link index.
pub(crate) const DISCOVERY_MAX_BODY_BYTES: u64 = 512 * 1024;

/// Body cap for `sitemap.xml`. The sitemap protocol ceiling is 50 MB uncompressed, so the
/// cap must be generous enough not to drop large-but-valid sitemaps.
pub(crate) const SITEMAP_MAX_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Join `path` onto the origin of `parsed`, producing a correctly-formatted absolute URL.
///
/// `Url::join` with a leading-slash path replaces the path while preserving scheme, host,
/// and port — and crucially brackets IPv6 literals in the authority (`[::1]:8080`), which
/// `format!("{host}:{port}")` does NOT (`host_str()` returns the address without brackets,
/// yielding an invalid authority for IPv6 hosts).
pub(crate) fn join_origin_path(parsed: &Url, path: &str) -> Result<String, Box<dyn Error>> {
    // Strip any userinfo (`user:pass@`) so credentials never propagate into discovery
    // requests or logs — join only the origin (scheme://host:port) with `path`. The
    // setters only fail on cannot-be-a-base URLs, which http(s) origins never are.
    let mut origin = parsed.clone();
    let _ = origin.set_username("");
    let _ = origin.set_password(None);
    Ok(origin.join(path)?.to_string())
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    if is_permanent_dead_host_status(status) {
        return false;
    }
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

pub(crate) fn request_timeout_secs(cfg: &Config) -> u64 {
    cfg.request_timeout_ms
        .unwrap_or(30_000)
        .div_ceil(1000)
        .max(1)
}

/// Read a successful response body, optionally capped at `max_bytes`.
///
/// - `Some(cap)` → streamed read with a hard byte cap and lossy UTF-8 decode (fine for
///   llms.txt/sitemap, which are UTF-8 by spec; lossy is strictly safer than dropping the
///   whole doc on a stray byte). Oversized bodies return `None`.
/// - `None` → charset-aware, lossy, uncapped `resp.text()` — matches `main`'s behavior for
///   HTML page backfill and any caller that must not silently drop large or non-UTF8 bodies.
async fn read_body_capped(
    resp: reqwest::Response,
    url: &str,
    max_bytes: Option<u64>,
) -> Option<String> {
    let Some(cap) = max_bytes else {
        return match resp.text().await {
            Ok(text) => Some(text),
            Err(e) => {
                log_warn(&format!("command=fetch body read failed url={url}: {e}"));
                None
            }
        };
    };
    if resp.content_length().is_some_and(|len| len > cap) {
        log_warn(&format!(
            "command=fetch oversized body rejected (content-length) cap_bytes={cap} url={url}"
        ));
        return None;
    }
    let mut collected: Vec<u8> = Vec::new();
    let mut stream = resp;
    loop {
        match stream.chunk().await {
            Ok(Some(chunk)) => {
                if collected.len() as u64 + chunk.len() as u64 > cap {
                    log_warn(&format!(
                        "command=fetch oversized body rejected (mid-stream) cap_bytes={cap} url={url}"
                    ));
                    return None;
                }
                collected.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                log_warn(&format!("command=fetch stream error url={url}: {e}"));
                return None;
            }
        }
    }
    // Lossy decode: replace malformed bytes rather than dropping the entire document
    // (a regression vs reqwest::Response::text(), which decodes charset-aware/lossily).
    Some(String::from_utf8_lossy(&collected).into_owned())
}

pub(crate) async fn fetch_text_with_retry(
    client: &reqwest::Client,
    url: &str,
    retries: usize,
    backoff_ms: u64,
    max_bytes: Option<u64>,
) -> Option<String> {
    if validate_url(url).is_err() {
        return None;
    }
    let mut attempt = 0usize;
    loop {
        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return read_body_capped(resp, url, max_bytes).await;
                }
                if !should_retry_status(status) {
                    // True 404/non-retryable absence stays low-noise (no warn).
                    return None;
                }
                if attempt >= retries {
                    // Retryable status (429/5xx) that never recovered — worth a warn.
                    log_warn(&format!(
                        "command=fetch status={} retries exhausted url={url}",
                        status.as_u16()
                    ));
                    return None;
                }
            }
            Err(_) if attempt >= retries => {
                log_warn(&format!(
                    "command=fetch transport error, retries exhausted url={url}"
                ));
                return None;
            }
            Err(_) => {}
        }

        attempt = attempt.saturating_add(1);
        let exp = attempt.saturating_sub(1).min(20) as u32;
        let multiplier = 1u64 << exp;
        let delay_ms = backoff_ms.saturating_mul(multiplier).max(1);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

#[cfg(test)]
#[path = "sitemap_tests.rs"]
mod tests;

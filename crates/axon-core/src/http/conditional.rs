//! Cheap conditional HTTP probe for URL-change watches. A 304 means "definitely
//! unchanged"; any 2xx is "maybe changed" (caller confirms by diffing). Body
//! ignored — the scrape pipeline re-fetches only when needed.

use crate::http::no_redirect_http_client;
use crate::http::validate_url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    NotModified,
    Modified {
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Failed(String),
}

fn conditional_headers(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Vec<(String, String)> {
    // Validators can encode private resource state. Never transmit them over
    // cleartext HTTP; an unconditional probe still preserves HTTP watch support.
    if !url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https") {
        return Vec::new();
    }
    let mut h = Vec::new();
    if let Some(e) = etag {
        h.push(("if-none-match".into(), e.to_string()));
    }
    if let Some(lm) = last_modified {
        h.push(("if-modified-since".into(), lm.to_string()));
    }
    h
}

fn classify(status: u16, etag: Option<String>, last_modified: Option<String>) -> Probe {
    match status {
        304 => Probe::NotModified,
        200..=299 => Probe::Modified {
            etag,
            last_modified,
        },
        other => Probe::Failed(format!("conditional probe got HTTP {other}")),
    }
}

/// Issue a cheap conditional GET to detect whether `url` changed since the
/// snapshot's validators.
///
/// SSRF posture (checked in security review): the initial `url` is validated
/// here via `validate_url` before any request. The no-redirect shared client
/// also wires `SsrfBlockingResolver` to close the connect-time DNS-rebinding
/// TOCTOU window. Redirects are deliberately not followed: besides keeping the
/// probe on its validated origin, this prevents HTTPS conditional validators
/// from being forwarded to a cleartext redirect target.
pub async fn conditional_probe(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Probe {
    if let Err(e) = validate_url(url) {
        return Probe::Failed(format!("ssrf guard rejected request: {e}"));
    }
    let client = match no_redirect_http_client() {
        Ok(c) => c,
        Err(e) => return Probe::Failed(format!("http client unavailable: {e}")),
    };
    let mut req = client.get(url);
    for (k, v) in conditional_headers(url, etag, last_modified) {
        req = req.header(k, v);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(_) => return Probe::Failed("conditional probe request failed".to_string()),
    };
    let status = resp.status().as_u16();
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    };
    classify(status, header("etag"), header("last-modified"))
}

#[cfg(test)]
#[path = "conditional_tests.rs"]
mod tests;

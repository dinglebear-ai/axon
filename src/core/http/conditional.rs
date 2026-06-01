//! Cheap conditional HTTP probe for URL-change watches. A 304 means "definitely
//! unchanged"; any 2xx is "maybe changed" (caller confirms by diffing). Body
//! ignored — the scrape pipeline re-fetches only when needed.

use crate::core::http::http_client;
use crate::core::http::validate_url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    NotModified,
    Modified {
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Failed(String),
}

fn conditional_headers(etag: Option<&str>, last_modified: Option<&str>) -> Vec<(String, String)> {
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
/// here via `validate_url` before any request. The request is then issued via
/// the shared `http_client()`, whose `build_client_with_options` installs a
/// `reqwest::redirect::Policy::custom` that re-runs `validate_url` on *every*
/// redirect hop (rejecting any cross-host redirect to a blocked destination),
/// and in non-test builds wires `SsrfBlockingResolver` as the DNS resolver to
/// close the connect-time DNS-rebinding TOCTOU window. So a server redirect
/// after this initial validation cannot reach an unvalidated/blocked host — the
/// validated initial host is not silently swapped for an unvalidated fetch
/// target. We intentionally keep redirect-following (the same client the scrape
/// path uses) rather than disabling it, because each hop is independently
/// SSRF-guarded.
pub async fn conditional_probe(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Probe {
    if let Err(e) = validate_url(url) {
        return Probe::Failed(format!("ssrf guard rejected {url}: {e}"));
    }
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => return Probe::Failed(format!("http client unavailable: {e}")),
    };
    let mut req = client.get(url);
    for (k, v) in conditional_headers(etag, last_modified) {
        req = req.header(k, v);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return Probe::Failed(format!("conditional probe request failed: {e}")),
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

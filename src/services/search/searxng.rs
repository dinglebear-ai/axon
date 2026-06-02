//! SearXNG search backend for `research`.
//!
//! Queries a self-hosted SearXNG instance's JSON API
//! (`{searxng_url}/search?format=json`) instead of Tavily. Selected whenever
//! `cfg.searxng_url` (env `AXON_SEARXNG_URL`) is non-empty; otherwise `research`
//! falls back to the Tavily path. SearXNG must have the `json` output format
//! enabled in its `settings.yml` (`search.formats`), or the API returns 403.
//!
//! SSRF: the request goes through the shared `http_client()`, whose
//! `SsrfBlockingResolver` validates every resolved IP at connect time, so a
//! misconfigured `searxng_url` pointing at internal space is rejected there.

use crate::core::config::Config;
use crate::core::http::http_client;
use serde::Deserialize;
use spider_agent::TimeRange;
use std::error::Error;

/// One SearXNG result, normalized to the fields `research` consumes.
#[derive(Debug)]
pub(super) struct SearxHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxRow>,
}

#[derive(Deserialize)]
struct SearxRow {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

/// Map spider's `TimeRange` to SearXNG's `time_range` query value.
fn time_range_param(tr: TimeRange) -> Option<&'static str> {
    match tr {
        TimeRange::Day => Some("day"),
        TimeRange::Week => Some("week"),
        TimeRange::Month => Some("month"),
        TimeRange::Year => Some("year"),
        _ => None,
    }
}

/// Query SearXNG and return up to `count` normalized hits.
pub(super) async fn searxng_search(
    cfg: &Config,
    query: &str,
    count: usize,
    time_range: Option<TimeRange>,
) -> Result<Vec<SearxHit>, Box<dyn Error>> {
    let endpoint = format!("{}/search", cfg.searxng_url.trim_end_matches('/'));
    // Parse-time SSRF guard: the shared client's resolver is bypassed for literal
    // IP hosts, so a `searxng_url` like `http://127.0.0.1:8080` would otherwise
    // reach internal services. `validate_url` rejects loopback/private/blocked
    // hosts up front (honors the test-only ALLOW_LOOPBACK thread-local).
    crate::core::http::validate_url(&endpoint)
        .map_err(|e| -> Box<dyn Error> { format!("invalid AXON_SEARXNG_URL: {e}").into() })?;
    let client =
        http_client().map_err(|e| -> Box<dyn Error> { format!("http client: {e}").into() })?;

    let mut params: Vec<(&str, String)> =
        vec![("q", query.to_string()), ("format", "json".to_string())];
    if let Some(tr) = time_range.and_then(time_range_param) {
        params.push(("time_range", tr.to_string()));
    }

    let resp = client
        .get(&endpoint)
        .query(&params)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| -> Box<dyn Error> { format!("searxng request failed: {e}").into() })?
        .error_for_status()
        .map_err(|e| -> Box<dyn Error> {
            format!("searxng returned an error status: {e}").into()
        })?;

    let parsed: SearxResponse = resp.json().await.map_err(|e| -> Box<dyn Error> {
        format!(
            "searxng JSON decode failed (is the `json` output format enabled in settings.yml?): {e}"
        )
        .into()
    })?;

    Ok(parsed
        .results
        .into_iter()
        .filter(|r| !r.url.is_empty())
        .take(count)
        .map(|r| SearxHit {
            url: r.url,
            title: r.title,
            snippet: r.content,
        })
        .collect())
}

#[cfg(test)]
#[path = "searxng_tests.rs"]
mod tests;

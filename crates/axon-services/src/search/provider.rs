//! SearXNG/Tavily provider dispatch for `search`/`search_batch` and
//! `research`.
//!
//! Constructs the real `axon-adapters` provider
//! (`crates/axon-adapters/src/providers/{searxng_search,tavily_search}.rs`)
//! from [`Config`] and calls it, replacing the reqwest clients this crate
//! used to hand-roll (issue #298 WS-D). Backend selection is unchanged from
//! before the delegation: SearXNG whenever `cfg.searxng_url` is set,
//! otherwise Tavily.
//!
//! `search`/`search_batch` and `research` retain one behavioral asymmetry
//! from before this delegation: `research` retries a failed Tavily call up to
//! [`TAVILY_RESEARCH_MAX_ATTEMPTS`] times with backoff
//! ([`run_search_for_research`]); plain `search` does not
//! ([`run_search`] makes a single attempt). That split is preserved here,
//! not introduced by it.

use axon_adapters::boundary::SearchProvider;
use axon_adapters::providers::searxng_search::{SearxngSearchConfig, SearxngSearchProvider};
use axon_adapters::providers::tavily_search::TavilySearchProvider;
use axon_api::source::{MetadataMap, SearchRequest, SearchResultItem, SearchTimeRange};
use axon_core::config::Config;
use axon_core::logging::log_info;
use spider_agent::TimeRange;
use std::error::Error;
use std::time::Duration;

/// Max Tavily attempts (1 initial + 2 retries) for the `research` search
/// phase only — mirrors the pre-delegation `TAVILY_MAX_ATTEMPTS` constant
/// that used to live in `search/synthesis.rs`.
const TAVILY_RESEARCH_MAX_ATTEMPTS: u32 = 3;
/// Base backoff between `research`'s Tavily retries; doubles on each
/// subsequent failure.
const TAVILY_RESEARCH_BACKOFF_BASE: Duration = Duration::from_millis(750);

fn to_provider_time_range(tr: TimeRange) -> Option<SearchTimeRange> {
    match tr {
        TimeRange::Day => Some(SearchTimeRange::Day),
        TimeRange::Week => Some(SearchTimeRange::Week),
        TimeRange::Month => Some(SearchTimeRange::Month),
        TimeRange::Year => Some(SearchTimeRange::Year),
        // No axon caller ever constructs a custom range (see the doc comment
        // on `axon_api::source::SearchTimeRange`); dropped rather than erroring.
        TimeRange::Custom { .. } => None,
    }
}

fn build_request(
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
) -> SearchRequest {
    SearchRequest {
        query: query.to_string(),
        limit: limit as u32,
        offset: offset as u32,
        time_range: time_range.and_then(to_provider_time_range),
        metadata: MetadataMap::new(),
    }
}

fn searxng_provider(cfg: &Config) -> SearxngSearchProvider {
    SearxngSearchProvider::new(SearxngSearchConfig {
        base_url: cfg.searxng_url.clone(),
        timeout: Duration::from_millis(cfg.request_timeout_ms.unwrap_or(30_000)),
    })
}

/// Build the Tavily provider, first checking (and surfacing, with the same
/// per-op message `ensure_tavily_configured` has always produced) that
/// `cfg.tavily_api_key` is set.
fn tavily_provider(cfg: &Config, op: &str) -> Result<TavilySearchProvider, Box<dyn Error>> {
    super::ensure_tavily_configured(cfg, op)?;
    Ok(TavilySearchProvider::new(cfg.tavily_api_key.clone())?)
}

/// Run a single search call against the backend selected by `cfg`, with no
/// retry. Used by `search`/`search_batch`.
pub(super) async fn run_search(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
    op: &str,
) -> Result<Vec<SearchResultItem>, Box<dyn Error>> {
    let request = build_request(query, limit, offset, time_range);
    if !cfg.searxng_url.is_empty() {
        return Ok(searxng_provider(cfg).search(request).await?.results);
    }
    Ok(tavily_provider(cfg, op)?.search(request).await?.results)
}

/// Run a search for `research`: same backend selection as [`run_search`], but
/// the Tavily path retries on failure (see the module doc comment).
pub(super) async fn run_search_for_research(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    time_range: Option<TimeRange>,
    op: &str,
) -> Result<Vec<SearchResultItem>, Box<dyn Error>> {
    let request = build_request(query, limit, offset, time_range);
    if !cfg.searxng_url.is_empty() {
        return Ok(searxng_provider(cfg).search(request).await?.results);
    }
    let provider = tavily_provider(cfg, op)?;
    tavily_search_with_retry(&provider, request).await
}

async fn tavily_search_with_retry(
    provider: &TavilySearchProvider,
    request: SearchRequest,
) -> Result<Vec<SearchResultItem>, Box<dyn Error>> {
    let mut last_err: Option<String> = None;
    for attempt in 1..=TAVILY_RESEARCH_MAX_ATTEMPTS {
        match provider.search(request.clone()).await {
            Ok(result) => return Ok(result.results),
            Err(e) => {
                let err_text = e.to_string();
                if attempt == TAVILY_RESEARCH_MAX_ATTEMPTS {
                    last_err = Some(format!(
                        "tavily search failed after {attempt} attempts: {err_text}"
                    ));
                    break;
                }
                let backoff = TAVILY_RESEARCH_BACKOFF_BASE * 2u32.pow(attempt - 1);
                log_info(&format!(
                    "tavily search attempt={attempt} failed ({err_text}); retrying in {}ms",
                    backoff.as_millis()
                ));
                tokio::time::sleep(backoff).await;
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| "tavily search failed".to_string())
        .into())
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;

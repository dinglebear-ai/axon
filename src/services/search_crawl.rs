use crate::core::config::Config;
use crate::core::http::{normalize_url, validate_url_with_dns};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::search::search_batch;
use crate::services::types::SearchOptions;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;

/// Typed result returned by [`search_and_crawl`].
///
/// Contains Tavily search results plus the outcome of auto-enqueueing
/// one bounded crawl job per result URL.
pub struct SearchAndCrawlResult {
    pub results: Vec<Value>,
    pub crawl_jobs: Vec<SearchCrawlJob>,
    pub crawl_rejected: Vec<SearchCrawlRejection>,
    pub auto_crawl_status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SearchCrawlJob {
    pub url: String,
    pub job_id: String,
}

#[derive(Debug, Serialize)]
pub struct SearchCrawlRejection {
    pub url: Option<String>,
    pub position: Option<i64>,
    pub title: Option<String>,
    pub kind: SearchCrawlRejectionKind,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCrawlRejectionKind {
    DuplicateUrl,
    InvalidUrl,
    MissingUrl,
    QueueRejected,
}

/// Run a Tavily search and enqueue one bounded crawl job per result URL.
///
/// This is the canonical entry point for both the CLI and MCP search action.
/// Callers receive a typed result and decide their own UX (error on zero jobs,
/// include in JSON response, etc.) — this function never errors on partial
/// crawl failures.
pub async fn search_and_crawl(
    cfg: &Config,
    service_context: &ServiceContext,
    query: &str,
    opts: SearchOptions,
) -> Result<SearchAndCrawlResult, Box<dyn Error>> {
    let results = search_batch(cfg, &[query], opts, None).await?.results;
    let crawl_output = enqueue_search_crawls(cfg, service_context, &results).await;
    let auto_crawl_status = crawl_status(&results, &crawl_output);
    Ok(SearchAndCrawlResult {
        results,
        crawl_jobs: crawl_output.jobs,
        crawl_rejected: crawl_output.rejected,
        auto_crawl_status,
    })
}

// ── internals ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct CrawlOutput {
    jobs: Vec<SearchCrawlJob>,
    rejected: Vec<SearchCrawlRejection>,
}

fn crawl_config(cfg: &Config) -> Config {
    // SECURITY: clear headers so auth meant for the search caller is never
    // replayed against URLs returned by Tavily.
    let mut c = cfg.clone();
    c.max_pages = 200;
    c.max_depth = 10;
    c.discover_sitemaps = false;
    c.max_sitemaps = 0;
    c.custom_headers = Vec::new();
    c.url_whitelist = Vec::new();
    c
}

async fn enqueue_search_crawls(
    cfg: &Config,
    service_context: &ServiceContext,
    results: &[Value],
) -> CrawlOutput {
    let crawl_cfg = crawl_config(cfg);
    let mut output = CrawlOutput::default();
    let mut seen = HashSet::new();

    for result in results {
        let Some(url) = result["url"].as_str().filter(|u| !u.is_empty()) else {
            output.rejected.push(result_rejection(
                result,
                SearchCrawlRejectionKind::MissingUrl,
                "search result missing url",
            ));
            continue;
        };
        let normalized = normalize_url(url).into_owned();
        if !seen.insert(normalized.clone()) {
            output.rejected.push(rejection(
                Some(&normalized),
                None,
                None,
                SearchCrawlRejectionKind::DuplicateUrl,
                "duplicate search result URL",
            ));
            continue;
        }
        match enqueue_one(&crawl_cfg, service_context, &normalized).await {
            Ok(job) => output.jobs.push(job),
            Err(r) => output.rejected.push(r),
        }
    }

    output
}

async fn enqueue_one(
    crawl_cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
) -> Result<SearchCrawlJob, SearchCrawlRejection> {
    if let Err(e) = validate_url_with_dns(url).await {
        return Err(rejection(
            Some(url),
            None,
            None,
            SearchCrawlRejectionKind::InvalidUrl,
            e.to_string(),
        ));
    }

    let url_owned = url.to_string();
    match crawl_service::crawl_start_with_context(
        crawl_cfg,
        std::slice::from_ref(&url_owned),
        service_context,
        None,
    )
    .await
    {
        Ok(outcome) => {
            let Some(job) = outcome.result.jobs.first() else {
                return Err(rejection(
                    Some(url),
                    None,
                    None,
                    SearchCrawlRejectionKind::QueueRejected,
                    "crawl service returned no job id",
                ));
            };
            Ok(SearchCrawlJob {
                url: url_owned,
                job_id: job.job_id.clone(),
            })
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::warn!(url = %url, error = %reason, "search auto-index: enqueue failed");
            Err(rejection(
                Some(url),
                None,
                None,
                SearchCrawlRejectionKind::QueueRejected,
                reason,
            ))
        }
    }
}

fn rejection(
    url: Option<&str>,
    position: Option<i64>,
    title: Option<&str>,
    kind: SearchCrawlRejectionKind,
    reason: impl Into<String>,
) -> SearchCrawlRejection {
    SearchCrawlRejection {
        url: url.map(str::to_string),
        position,
        title: title.map(str::to_string),
        kind,
        reason: reason.into(),
    }
}

fn result_rejection(
    result: &Value,
    kind: SearchCrawlRejectionKind,
    reason: impl Into<String>,
) -> SearchCrawlRejection {
    rejection(
        result["url"].as_str(),
        result["position"].as_i64(),
        result["title"].as_str(),
        kind,
        reason,
    )
}

fn crawl_status(results: &[Value], output: &CrawlOutput) -> &'static str {
    if results.is_empty() {
        "no_results"
    } else if output.jobs.is_empty() {
        "failed"
    } else if output.rejected.is_empty() {
        "queued"
    } else {
        "partial"
    }
}

#[cfg(test)]
#[path = "search_crawl_tests.rs"]
pub(crate) mod tests;

use crate::context::ServiceContext;
use crate::search::search_batch;
use crate::search_source_index::enqueue_web_source_auto_index;
use crate::types::{ResearchHit, SearchOptions};
use axon_api::source::{LifecycleStatus, SourceScope};
use axon_core::config::Config;
use axon_core::http::{normalize_url, validate_url_with_dns};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;

use crate::search::SearchError;

/// Typed result returned by [`search_and_index_sources`].
///
/// Contains search results plus the outcome of auto-enqueueing
/// one bounded Source job per result URL.
pub struct SearchAndSourceIndexResult {
    pub results: Vec<Value>,
    pub source_jobs: Vec<SearchSourceJob>,
    pub source_jobs_rejected: Vec<SearchSourceRejection>,
    pub source_index_status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct SearchSourceJob {
    pub url: String,
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct SearchSourceRejection {
    pub url: Option<String>,
    pub position: Option<i64>,
    pub title: Option<String>,
    pub kind: SearchSourceRejectionKind,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSourceRejectionKind {
    DuplicateUrl,
    InvalidUrl,
    MissingUrl,
    QueueRejected,
    WaitFailed,
}

/// Run a SearXNG/Tavily search and enqueue one bounded Source job per result URL.
///
/// This is the canonical entry point for both the CLI and MCP search action.
/// Callers receive a typed result and decide their own UX (error on zero jobs,
/// include in JSON response, etc.) — this function never errors on partial
/// auto-index failures.
pub async fn search_and_index_sources(
    cfg: &Config,
    service_context: &ServiceContext,
    query: &str,
    opts: SearchOptions,
) -> Result<SearchAndSourceIndexResult, SearchError> {
    let results = search_batch(cfg, &[query], opts, None).await?.results;
    let source_output = enqueue_search_sources(cfg, service_context, &results).await;
    let source_index_status = source_index_status(&results, &source_output);
    Ok(SearchAndSourceIndexResult {
        results,
        source_jobs: source_output.jobs,
        source_jobs_rejected: source_output.rejected,
        source_index_status,
    })
}

// ── internals ────────────────────────────────────────────────────────────────

#[derive(Default)]
pub(crate) struct SourceIndexOutput {
    pub(crate) jobs: Vec<SearchSourceJob>,
    pub(crate) rejected: Vec<SearchSourceRejection>,
}

fn auto_index_config(cfg: &Config) -> Config {
    // SECURITY: clear headers so auth meant for the search caller is never
    // replayed against URLs returned by the configured search backend.
    let mut c = cfg.clone();
    // Search auto-indexing must enqueue every accepted result URL before any
    // optional wait phase.
    c.wait = false;
    c.max_pages = 1;
    c.max_depth = 0;
    c.discover_sitemaps = false;
    c.max_sitemaps = 0;
    c.custom_headers = Vec::new();
    c.url_whitelist = Vec::new();
    c
}

async fn enqueue_search_sources(
    cfg: &Config,
    service_context: &ServiceContext,
    results: &[Value],
) -> SourceIndexOutput {
    enqueue_search_sources_with_reason(cfg, service_context, results, "search").await
}

async fn enqueue_search_sources_with_reason(
    cfg: &Config,
    service_context: &ServiceContext,
    results: &[Value],
    reason: &str,
) -> SourceIndexOutput {
    let auto_index_cfg = auto_index_config(cfg);
    let mut output = SourceIndexOutput::default();
    let mut seen = HashSet::new();

    for result in results {
        let Some(url) = result["url"].as_str().filter(|u| !u.is_empty()) else {
            output.rejected.push(result_rejection(
                result,
                SearchSourceRejectionKind::MissingUrl,
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
                SearchSourceRejectionKind::DuplicateUrl,
                "duplicate search result URL",
            ));
            continue;
        }
        match enqueue_one(&auto_index_cfg, service_context, &normalized, reason).await {
            Ok(job) => output.jobs.push(job),
            Err(r) => output.rejected.push(r),
        }
    }

    if cfg.wait && !output.jobs.is_empty() {
        wait_for_queued_source_jobs(service_context, &mut output).await;
    }

    output
}

pub(crate) async fn enqueue_research_sources(
    cfg: &Config,
    service_context: &ServiceContext,
    hits: &[ResearchHit],
) -> SourceIndexOutput {
    let results: Vec<Value> = hits
        .iter()
        .map(|hit| {
            serde_json::json!({
                "url": hit.url,
                "title": hit.title,
                "position": hit.position,
                "snippet": hit.snippet,
            })
        })
        .collect();
    enqueue_search_sources_with_reason(cfg, service_context, &results, "research").await
}

async fn enqueue_one(
    auto_index_cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
    reason: &str,
) -> Result<SearchSourceJob, SearchSourceRejection> {
    if let Err(e) = validate_url_with_dns(url).await {
        return Err(rejection(
            Some(url),
            None,
            None,
            SearchSourceRejectionKind::InvalidUrl,
            e.to_string(),
        ));
    }

    let url_owned = url.to_string();
    match enqueue_web_source_auto_index(
        auto_index_cfg,
        service_context,
        &url_owned,
        SourceScope::Page,
        1,
        0,
        auto_index_cfg.embed,
        reason,
    )
    .await
    {
        Ok(job) => Ok(SearchSourceJob {
            url: url_owned,
            job_id: job.id.0.to_string(),
        }),
        Err(e) => {
            let reason = e.to_string();
            tracing::warn!(url = %url, error = %reason, "search source auto-index: enqueue failed");
            Err(rejection(
                Some(url),
                None,
                None,
                SearchSourceRejectionKind::QueueRejected,
                reason,
            ))
        }
    }
}

async fn wait_for_queued_source_jobs(
    service_context: &ServiceContext,
    output: &mut SourceIndexOutput,
) {
    for job in &output.jobs {
        let Ok(job_id) = uuid::Uuid::parse_str(&job.job_id) else {
            output.rejected.push(rejection(
                Some(&job.url),
                None,
                None,
                SearchSourceRejectionKind::WaitFailed,
                format!("source auto-index returned invalid job id: {}", job.job_id),
            ));
            continue;
        };

        match wait_for_unified_source_job(service_context, job_id).await {
            Ok(status)
                if status == LifecycleStatus::Failed || status == LifecycleStatus::Canceled =>
            {
                let mut reason = format!("source job {job_id} {status:?}");
                if let Ok(Some(summary)) = crate::jobs::unified_job_status(
                    service_context,
                    axon_api::source::JobId(job_id),
                )
                .await
                {
                    if let Some(error) = summary.last_error {
                        reason.push_str(": ");
                        reason.push_str(&error.message);
                    }
                }
                output.rejected.push(rejection(
                    Some(&job.url),
                    None,
                    None,
                    SearchSourceRejectionKind::WaitFailed,
                    reason,
                ));
            }
            Ok(_) => {}
            Err(e) => output.rejected.push(rejection(
                Some(&job.url),
                None,
                None,
                SearchSourceRejectionKind::WaitFailed,
                e.to_string(),
            )),
        }
    }
}

/// Poll the unified job store for `job_id`'s terminal `LifecycleStatus`,
/// mirroring `ServiceJobRuntime::wait_for_job`'s timeout semantics
/// (`cfg.job_wait_timeout_secs`) but reading the unified store instead of a
/// legacy per-family table.
async fn wait_for_unified_source_job(
    service_context: &ServiceContext,
    job_id: uuid::Uuid,
) -> Result<LifecycleStatus, Box<dyn Error + Send + Sync>> {
    let timeout_secs = service_context.cfg.job_wait_timeout_secs;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let summary =
            crate::jobs::unified_job_status(service_context, axon_api::source::JobId(job_id))
                .await
                .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        if let Some(summary) = summary
            && matches!(
                summary.status,
                LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
                    | LifecycleStatus::Failed
                    | LifecycleStatus::Canceled
                    | LifecycleStatus::Expired
                    | LifecycleStatus::Skipped
            )
        {
            return Ok(summary.status);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("source job {job_id} wait timed out after {timeout_secs}s").into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

fn rejection(
    url: Option<&str>,
    position: Option<i64>,
    title: Option<&str>,
    kind: SearchSourceRejectionKind,
    reason: impl Into<String>,
) -> SearchSourceRejection {
    SearchSourceRejection {
        url: url.map(str::to_string),
        position,
        title: title.map(str::to_string),
        kind,
        reason: reason.into(),
    }
}

fn result_rejection(
    result: &Value,
    kind: SearchSourceRejectionKind,
    reason: impl Into<String>,
) -> SearchSourceRejection {
    rejection(
        result["url"].as_str(),
        result["position"].as_i64(),
        result["title"].as_str(),
        kind,
        reason,
    )
}

fn source_index_status(results: &[Value], output: &SourceIndexOutput) -> &'static str {
    if results.is_empty() {
        "no_results"
    } else if output.jobs.is_empty() {
        "failed"
    } else if output
        .rejected
        .iter()
        .any(|r| matches!(r.kind, SearchSourceRejectionKind::WaitFailed))
    {
        let wait_failures = output
            .rejected
            .iter()
            .filter(|r| matches!(r.kind, SearchSourceRejectionKind::WaitFailed))
            .count();
        if wait_failures >= output.jobs.len() {
            "wait_failed"
        } else {
            "partial_wait_failed"
        }
    } else if output.rejected.is_empty() {
        "queued"
    } else {
        "partial"
    }
}

pub(crate) fn source_index_status_for_output<T>(
    results: &[T],
    output: &SourceIndexOutput,
) -> &'static str {
    if results.is_empty() {
        return "no_results";
    }
    if output.jobs.is_empty() {
        return "failed";
    }
    let wait_failures = output
        .rejected
        .iter()
        .filter(|r| matches!(r.kind, SearchSourceRejectionKind::WaitFailed))
        .count();
    if wait_failures == output.jobs.len() {
        "wait_failed"
    } else if wait_failures > 0 {
        "partial_wait_failed"
    } else if output.rejected.is_empty() {
        "queued"
    } else {
        "partial"
    }
}

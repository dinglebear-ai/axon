//! Job-queue status aggregation for the `axon status` command.

use crate::core::config::Config;
use crate::jobs::backend::JobKind;
use crate::services::context::ServiceContext;
use crate::services::jobs as job_service;
use crate::services::system::watchdog::{include_status_job, include_status_view};
use crate::services::types::{ServiceJob, StatusResult, StatusTotals};
use std::error::Error;

pub struct StatusJobs {
    pub crawl: Vec<ServiceJob>,
    pub extract: Vec<ServiceJob>,
    pub embed: Vec<ServiceJob>,
    pub ingest: Vec<ServiceJob>,
}

#[must_use = "full_status returns a Result that should be handled"]
pub async fn full_status(service_context: &ServiceContext) -> Result<StatusResult, Box<dyn Error>> {
    let (jobs, totals, errors) = load_status_jobs(service_context).await?;
    let payload = build_status_payload_with_errors(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
        &errors,
    );
    let mut text = vec![
        "Axon Status".to_string(),
        format!("crawl jobs:   {} total", totals.crawl),
        format!("extract jobs: {} total", totals.extract),
        format!("embed jobs:   {} total", totals.embed),
        format!("ingest jobs:  {} total", totals.ingest),
    ];
    if !errors.is_empty() {
        text.push(format!(
            "degraded: {} status count error{}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        ));
    }
    Ok(StatusResult {
        payload,
        text: text.join("\n"),
        totals,
        degraded: !errors.is_empty(),
        errors,
    })
}

/// Filter + view-mode in one pass: drop reclaimed/non-reclaimed jobs, then
/// apply the active-only / recent-only view mode.
fn filter_and_view<T>(
    cfg: &Config,
    jobs: Vec<T>,
    status_of: impl Fn(&T) -> &str,
    error_of: impl Fn(&T) -> Option<&str>,
) -> Vec<T> {
    let reclaimed_only = cfg.reclaimed_status_only;
    let active_only = cfg.active_status_only;
    let recent_only = cfg.recent_status_only;
    jobs.into_iter()
        .filter(|job| include_status_job(status_of(job), error_of(job), reclaimed_only))
        .filter(|job| include_status_view(status_of(job), active_only, recent_only))
        .collect()
}

pub async fn load_status_jobs(
    service_context: &ServiceContext,
) -> Result<(StatusJobs, StatusTotals, Vec<String>), Box<dyn Error>> {
    let cfg = service_context.cfg.as_ref();
    let (
        crawl_raw,
        extract_raw,
        embed_raw,
        ingest_raw,
        crawl_total,
        extract_total,
        embed_total,
        ingest_total,
    ) = tokio::join!(
        async {
            job_service::list_jobs(service_context, JobKind::Crawl, 20, 0)
                .await
                .map_err(|e| format!("crawl: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Extract, 20, 0)
                .await
                .map_err(|e| format!("extract: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Embed, 20, 0)
                .await
                .map_err(|e| format!("embed: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Ingest, 20, 0)
                .await
                .map_err(|e| format!("ingest: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Crawl)
                .await
                .map_err(|e| format!("crawl: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Extract)
                .await
                .map_err(|e| format!("extract: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Embed)
                .await
                .map_err(|e| format!("embed: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Ingest)
                .await
                .map_err(|e| format!("ingest: {e}"))
        },
    );

    let mut errors = Vec::new();
    let jobs = StatusJobs {
        crawl: list_or_degraded("crawl", crawl_raw, cfg, &mut errors),
        extract: list_or_degraded("extract", extract_raw, cfg, &mut errors),
        embed: list_or_degraded("embed", embed_raw, cfg, &mut errors),
        ingest: list_or_degraded("ingest", ingest_raw, cfg, &mut errors),
    };
    let totals = StatusTotals {
        crawl: count_or_degraded("crawl", crawl_total, &mut errors),
        extract: count_or_degraded("extract", extract_total, &mut errors),
        embed: count_or_degraded("embed", embed_total, &mut errors),
        ingest: count_or_degraded("ingest", ingest_total, &mut errors),
    };
    Ok((jobs, totals, errors))
}

fn count_or_degraded(
    kind: &'static str,
    result: Result<i64, String>,
    errors: &mut Vec<String>,
) -> i64 {
    match result {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(kind, %error, "status_count_jobs_degraded");
            errors.push(error);
            0
        }
    }
}

fn list_or_degraded(
    kind: &'static str,
    result: Result<Vec<ServiceJob>, String>,
    cfg: &Config,
    errors: &mut Vec<String>,
) -> Vec<ServiceJob> {
    match result {
        Ok(jobs) => filter_and_view(cfg, jobs, |j| &j.status, |j| j.error_text.as_deref()),
        Err(error) => {
            tracing::warn!(kind, %error, "status_list_jobs_degraded");
            errors.push(error);
            vec![]
        }
    }
}

pub fn build_status_payload(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    totals: &StatusTotals,
) -> serde_json::Value {
    build_status_payload_with_errors(
        crawl_jobs,
        extract_jobs,
        embed_jobs,
        ingest_jobs,
        totals,
        &[],
    )
}

pub fn build_status_payload_with_errors(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    totals: &StatusTotals,
    errors: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "local_crawl_jobs": crawl_jobs,
        "local_extract_jobs": extract_jobs,
        "local_embed_jobs": embed_jobs,
        "local_ingest_jobs": ingest_jobs,
        "totals": {
            "crawl": totals.crawl,
            "extract": totals.extract,
            "embed": totals.embed,
            "ingest": totals.ingest,
        },
        "degraded": !errors.is_empty(),
        "errors": errors,
    })
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

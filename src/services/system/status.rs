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
    let (jobs, totals) = load_status_jobs(service_context).await?;
    let payload = build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
    );
    let text = [
        "Axon Status".to_string(),
        format!("crawl jobs:   {} total", totals.crawl),
        format!("extract jobs: {} total", totals.extract),
        format!("embed jobs:   {} total", totals.embed),
        format!("ingest jobs:  {} total", totals.ingest),
    ]
    .join("\n");
    Ok(StatusResult {
        payload,
        text,
        totals,
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
) -> Result<(StatusJobs, StatusTotals), Box<dyn Error>> {
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
                .unwrap_or_else(|e| {
                    tracing::warn!(kind = "crawl", error = %e, "status: count_jobs failed, defaulting to 0");
                    0
                })
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Extract)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(kind = "extract", error = %e, "status: count_jobs failed, defaulting to 0");
                    0
                })
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Embed)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(kind = "embed", error = %e, "status: count_jobs failed, defaulting to 0");
                    0
                })
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Ingest)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(kind = "ingest", error = %e, "status: count_jobs failed, defaulting to 0");
                    0
                })
        },
    );

    let jobs = StatusJobs {
        crawl: filter_and_view(cfg, crawl_raw?, |j| &j.status, |j| j.error_text.as_deref()),
        extract: filter_and_view(
            cfg,
            extract_raw?,
            |j| &j.status,
            |j| j.error_text.as_deref(),
        ),
        embed: filter_and_view(cfg, embed_raw?, |j| &j.status, |j| j.error_text.as_deref()),
        ingest: filter_and_view(cfg, ingest_raw?, |j| &j.status, |j| j.error_text.as_deref()),
    };
    let totals = StatusTotals {
        crawl: crawl_total,
        extract: extract_total,
        embed: embed_total,
        ingest: ingest_total,
    };
    Ok((jobs, totals))
}

pub fn build_status_payload(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    totals: &StatusTotals,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_payload_includes_expected_keys() {
        let payload = build_status_payload(&[], &[], &[], &[], &StatusTotals::default());
        assert!(payload.get("local_crawl_jobs").is_some());
        assert!(payload.get("local_ingest_jobs").is_some());
        assert!(payload.get("totals").is_some());
    }
}

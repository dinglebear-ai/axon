//! Job-queue status aggregation for the `axon status` command.

use crate::context::ServiceContext;
use crate::jobs as job_service;
use crate::system::watchdog::{include_status_job, include_status_view};
use crate::types::{ServiceJob, StatusResult, StatusTotals};
use axon_api::service_job::StatusJob;
use axon_api::source::JobKind;
use axon_core::config::Config;
use axon_jobs::store::sqlite_diagnostics;
use std::error::Error;

pub struct StatusJobs {
    pub source: Vec<ServiceJob>,
    pub extract: Vec<ServiceJob>,
    pub watch: Vec<ServiceJob>,
    pub prune: Vec<ServiceJob>,
}

#[must_use = "full_status returns a Result that should be handled"]
pub async fn full_status(service_context: &ServiceContext) -> Result<StatusResult, Box<dyn Error>> {
    let (jobs, totals, mut errors) = load_status_jobs(service_context).await?;
    let sqlite = sqlite_diagnostics(&service_context.cfg.sqlite_path).await;
    if let Some(error) = sqlite_status_error(&sqlite) {
        errors.push(error);
    }
    let payload = build_status_payload_with_errors_and_sqlite(
        &jobs.source,
        &jobs.extract,
        &jobs.watch,
        &jobs.prune,
        &totals,
        &errors,
        &sqlite,
    );
    let mut text = vec![
        "Axon Status".to_string(),
        format!("source jobs:  {} total", totals.source),
        format!("extract jobs: {} total", totals.extract),
        format!("watch jobs:   {} total", totals.watch),
        format!("prune jobs:   {} total", totals.prune),
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
        warnings: errors,
    })
}

pub fn sqlite_status_error(sqlite: &serde_json::Value) -> Option<String> {
    if sqlite.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        return None;
    }
    let ioerr_count = sqlite
        .get("runtime_ioerr_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if ioerr_count > 0 {
        return Some(format!(
            "SQLite runtime IOERR detected ({ioerr_count} error{}); restart the Axon process after verifying storage",
            if ioerr_count == 1 { "" } else { "s" }
        ));
    }
    let quick_check = sqlite
        .get("quick_check")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    Some(format!("SQLite quick_check is {quick_check}"))
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
        source_raw,
        extract_raw,
        watch_raw,
        prune_raw,
        source_total,
        extract_total,
        watch_total,
        prune_total,
    ) = tokio::join!(
        async {
            job_service::list_jobs(service_context, JobKind::Source, 20, 0)
                .await
                .map_err(|e| format!("source: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Extract, 20, 0)
                .await
                .map_err(|e| format!("extract: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Watch, 20, 0)
                .await
                .map_err(|e| format!("watch: {e}"))
        },
        async {
            job_service::list_jobs(service_context, JobKind::Prune, 20, 0)
                .await
                .map_err(|e| format!("prune: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Source)
                .await
                .map_err(|e| format!("source: {e}"))
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
                .count_jobs(JobKind::Watch)
                .await
                .map_err(|e| format!("watch: {e}"))
        },
        async {
            service_context
                .jobs
                .count_jobs(JobKind::Prune)
                .await
                .map_err(|e| format!("prune: {e}"))
        },
    );

    let mut errors = Vec::new();
    let jobs = StatusJobs {
        source: list_or_degraded("source", source_raw, cfg, &mut errors),
        extract: list_or_degraded("extract", extract_raw, cfg, &mut errors),
        watch: list_or_degraded("watch", watch_raw, cfg, &mut errors),
        prune: list_or_degraded("prune", prune_raw, cfg, &mut errors),
    };
    let totals = StatusTotals {
        source: count_or_degraded("source", source_total, &mut errors),
        extract: count_or_degraded("extract", extract_total, &mut errors),
        watch: count_or_degraded("watch", watch_total, &mut errors),
        prune: count_or_degraded("prune", prune_total, &mut errors),
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
    source_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    watch_jobs: &[ServiceJob],
    prune_jobs: &[ServiceJob],
    totals: &StatusTotals,
) -> serde_json::Value {
    build_status_payload_with_errors(
        source_jobs,
        extract_jobs,
        watch_jobs,
        prune_jobs,
        totals,
        &[],
    )
}

pub fn build_status_payload_with_errors(
    source_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    watch_jobs: &[ServiceJob],
    prune_jobs: &[ServiceJob],
    totals: &StatusTotals,
    errors: &[String],
) -> serde_json::Value {
    build_status_payload_with_errors_and_sqlite(
        source_jobs,
        extract_jobs,
        watch_jobs,
        prune_jobs,
        totals,
        errors,
        &serde_json::Value::Null,
    )
}

pub fn build_status_payload_with_errors_and_sqlite(
    source_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    watch_jobs: &[ServiceJob],
    prune_jobs: &[ServiceJob],
    totals: &StatusTotals,
    errors: &[String],
    sqlite: &serde_json::Value,
) -> serde_json::Value {
    let jobs = status_jobs(JobKind::Source, source_jobs)
        .into_iter()
        .chain(status_jobs(JobKind::Extract, extract_jobs))
        .collect();
    let payload = StatusPayload {
        jobs,
        watches: status_jobs(JobKind::Watch, watch_jobs),
        cleanup: StatusCleanup {
            jobs: status_jobs(JobKind::Prune, prune_jobs),
        },
        totals: totals.clone(),
        sqlite: sqlite.clone(),
        degraded: !errors.is_empty(),
        warnings: errors.to_vec(),
    };
    serde_json::to_value(payload).expect("StatusPayload serialization is infallible")
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusPayload {
    jobs: Vec<StatusJob>,
    watches: Vec<StatusJob>,
    cleanup: StatusCleanup,
    totals: StatusTotals,
    sqlite: serde_json::Value,
    degraded: bool,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusCleanup {
    jobs: Vec<StatusJob>,
}

fn status_jobs(kind: JobKind, jobs: &[ServiceJob]) -> Vec<StatusJob> {
    jobs.iter()
        .map(|job| StatusJob::from_service_job(kind, job))
        .collect()
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

//! Extract-only bridge from the unified `JobStore` onto the legacy
//! `ServiceJob` shape.
//!
//! `JobKind::Extract` now enqueues and executes on the unified job store
//! (`crates/axon-jobs/src/workers/unified/extract_runner.rs`), but every
//! CLI/MCP/REST caller still renders through `ServiceJob` (shared with
//! Crawl/Embed/Ingest, which remain on the legacy per-family backend). This
//! module is the one place that converts unified `JobSummary`/result DTOs
//! into `ServiceJob` so those shared renderers keep working unchanged for
//! Extract while other kinds still take the legacy path in
//! [`super::sqlite::SqliteServiceRuntime`].

use std::error::Error;
use std::sync::Arc;

use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobId, JobKind as UnifiedJobKind, JobListRequest,
    JobRecoveryRequest, JobSummary, LifecycleStatus,
};
use axon_jobs::boundary::JobStore;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::ServiceJob;

fn parse_timestamp(value: &axon_api::source::Timestamp) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value.0)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// Collapse the 11-value unified `LifecycleStatus` onto the 5-value legacy
/// status vocabulary `ServiceJob` callers already render against.
fn legacy_status(status: LifecycleStatus) -> String {
    match status {
        LifecycleStatus::Queued
        | LifecycleStatus::Pending
        | LifecycleStatus::Waiting
        | LifecycleStatus::Blocked => "pending",
        LifecycleStatus::Running | LifecycleStatus::Canceling => "running",
        LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded => "completed",
        LifecycleStatus::Failed | LifecycleStatus::Expired => "failed",
        LifecycleStatus::Canceled | LifecycleStatus::Skipped => "canceled",
    }
    .to_string()
}

/// Extract's stored `request` payload shape:
/// `{"urls": [...], "config_json": "..."}` (see
/// `extract_start_with_context` in `crates/axon-services/src/extract.rs`).
/// Pulls just the `urls` array back out so the bridge can populate
/// `ServiceJob.urls_json`/`target` for CLI/MCP/REST rendering.
fn urls_from_request_json(request_json: &serde_json::Value) -> Option<serde_json::Value> {
    request_json.get("urls").cloned()
}

fn job_summary_to_service_job(
    summary: JobSummary,
    request_json: Option<serde_json::Value>,
) -> ServiceJob {
    let urls_json = request_json.as_ref().and_then(urls_from_request_json);
    let target = urls_json.as_ref().and_then(|urls| match urls {
        serde_json::Value::Array(items) if items.len() == 1 => {
            items.first().and_then(|v| v.as_str()).map(String::from)
        }
        _ => None,
    });
    ServiceJob {
        id: summary.job_id.0,
        status: legacy_status(summary.status),
        created_at: parse_timestamp(&summary.created_at),
        updated_at: parse_timestamp(&summary.updated_at),
        started_at: summary.started_at.as_ref().map(parse_timestamp),
        finished_at: summary.finished_at.as_ref().map(parse_timestamp),
        error_text: summary.last_error.as_ref().map(|e| e.message.clone()),
        url: None,
        source_type: None,
        target,
        urls_json,
        progress_json: summary
            .counts
            .as_ref()
            .and_then(|counts| serde_json::to_value(counts).ok()),
        result_json: None,
        config_json: None,
        attempt_count: summary.attempt.max(1) as i64,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

pub(super) async fn list(
    store: &Arc<dyn JobStore>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
    // The unified store paginates by opaque cursor, not offset; Extract job
    // volumes are low enough that re-listing from the start and slicing is
    // an acceptable bridge until Extract's own cursor-aware CLI/MCP/REST
    // rendering lands.
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(UnifiedJobKind::Extract),
            source_id: None,
            watch_id: None,
            limit: Some((offset + limit).clamp(1, 1000) as u32),
            cursor: None,
        })
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    let page_items = page
        .items
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect::<Vec<_>>();
    // One `request_json` lookup per rendered row: the unified `JobStore`
    // trait's `list()` returns only `JobSummary` (no request payload), and
    // Extract job volumes are low enough (see the offset workaround above)
    // that this N+1 is an acceptable bridge cost until Extract gets its own
    // cursor-aware CLI/MCP/REST rendering.
    let mut jobs = Vec::with_capacity(page_items.len());
    for summary in page_items {
        let job_id = summary.job_id;
        let request_json = store
            .request_json(job_id)
            .await
            .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
        jobs.push(job_summary_to_service_job(summary, request_json));
    }
    Ok(jobs)
}

pub(super) async fn status(
    store: &Arc<dyn JobStore>,
    id: Uuid,
) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
    let job_id = JobId::new(id);
    let summary = store
        .get(job_id)
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    let Some(summary) = summary else {
        return Ok(None);
    };
    let request_json = store
        .request_json(job_id)
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(Some(job_summary_to_service_job(summary, request_json)))
}

pub(super) async fn cancel(
    store: &Arc<dyn JobStore>,
    id: Uuid,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let result = store
        .cancel(
            JobId::new(id),
            JobCancelRequest {
                reason: Some("cancel requested via extract job surface".to_string()),
                force_after_ms: None,
            },
        )
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(matches!(
        result.status,
        LifecycleStatus::Canceling | LifecycleStatus::Canceled
    ))
}

pub(super) async fn cleanup(
    store: &Arc<dyn JobStore>,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let result = store
        .cleanup(JobCleanupRequest {
            dry_run: false,
            kind: Some(UnifiedJobKind::Extract),
            older_than: None,
            status: None,
            limit: Some(500),
            older_than_seconds: None,
            confirm_all_terminal: true,
        })
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(result.deleted)
}

pub(super) async fn clear(store: &Arc<dyn JobStore>) -> Result<u64, Box<dyn Error + Send + Sync>> {
    // `clear` is the CLI's "purge the whole queue" verb — same as cleanup for
    // the unified backend since both drain via the same terminal-job sweep.
    cleanup(store).await
}

pub(super) async fn recover(
    store: &Arc<dyn JobStore>,
    stale_threshold_ms: i64,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let stale_before = axon_api::source::Timestamp::from(
        Utc::now() - chrono::Duration::milliseconds(stale_threshold_ms.max(0)),
    );
    let result = store
        .recover(JobRecoveryRequest {
            kind: Some(UnifiedJobKind::Extract),
            stale_before: Some(stale_before),
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(result.recovered)
}

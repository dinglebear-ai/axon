//! Historical crawl bridge from the unified `JobStore` onto the legacy
//! `ServiceJob` shape.
//!
//! Web acquisition now submits detached `JobKind::Source` rows carrying
//! `SourceRequest { scope: site }`. This module remains only to render,
//! cancel, cleanup, and dead-letter legacy `JobKind::Crawl` rows that may
//! already exist in a jobs database.

use std::error::Error;
use std::sync::Arc;

use axon_api::source::{
    ApiError, JobCancelRequest, JobCleanupRequest, JobId, JobKind as UnifiedJobKind,
    JobListRequest, JobStatusUpdate, JobSummary, LifecycleStatus, PipelinePhase, Severity,
    SourceError,
};
use axon_jobs::boundary::JobStore;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::ServiceJob;

const LEGACY_CRAWL_REMOVED_CODE: &str = "legacy.crawl.removed";
const LEGACY_CRAWL_REMOVED_RETRY: &str =
    "legacy crawl jobs were removed; re-run as `axon <url> --scope site`";

const ACTIVE_LEGACY_CRAWL_STATUSES: &[LifecycleStatus] = &[
    LifecycleStatus::Queued,
    LifecycleStatus::Pending,
    LifecycleStatus::Blocked,
    LifecycleStatus::Running,
    LifecycleStatus::Waiting,
    LifecycleStatus::Canceling,
];

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

/// Legacy crawl's stored `request` payload shape: `{"urls": [<one url>],
/// "config_json": "..."}`. Pulls the URL back out so the bridge can populate
/// `ServiceJob.url`/`urls_json`/`target` for CLI/MCP/REST rendering.
fn url_from_request_json(request_json: &serde_json::Value) -> Option<(String, serde_json::Value)> {
    let urls_json = request_json.get("urls").cloned()?;
    let url = urls_json
        .as_array()
        .and_then(|items| items.first())
        .and_then(|v| v.as_str())
        .map(String::from)?;
    Some((url, urls_json))
}

fn job_summary_to_service_job(
    summary: JobSummary,
    request_json: Option<serde_json::Value>,
) -> ServiceJob {
    let (url, urls_json) = request_json
        .as_ref()
        .and_then(url_from_request_json)
        .map(|(url, urls_json)| (Some(url.clone()), Some(urls_json)))
        .unwrap_or((None, None));
    ServiceJob {
        id: summary.job_id.0,
        status: legacy_status(summary.status),
        created_at: parse_timestamp(&summary.created_at),
        updated_at: parse_timestamp(&summary.updated_at),
        started_at: summary.started_at.as_ref().map(parse_timestamp),
        finished_at: summary.finished_at.as_ref().map(parse_timestamp),
        error_text: summary.last_error.as_ref().map(|e| e.message.clone()),
        url: url.clone(),
        source_type: None,
        target: url,
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
    // The unified store paginates by opaque cursor, not offset; re-list from
    // the start and slice, matching extract_bridge::list's bridge shape.
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(UnifiedJobKind::Crawl),
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
                reason: Some("cancel requested via crawl job surface".to_string()),
                force_after_ms: None,
                actor: None,
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
            kind: Some(UnifiedJobKind::Crawl),
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
    _stale_threshold_ms: i64,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let mut failed = 0_u64;
    for status in ACTIVE_LEGACY_CRAWL_STATUSES {
        let mut cursor = None;
        loop {
            let page = store
                .list(JobListRequest {
                    status: Some(*status),
                    kind: Some(UnifiedJobKind::Crawl),
                    source_id: None,
                    watch_id: None,
                    limit: Some(1000),
                    cursor: cursor.take(),
                })
                .await
                .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
            for summary in page.items {
                if mark_legacy_crawl_removed(store.as_ref(), summary.job_id)
                    .await
                    .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?
                {
                    failed += 1;
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    Ok(failed)
}

pub(super) async fn mark_legacy_crawl_removed(
    store: &dyn JobStore,
    job_id: JobId,
) -> Result<bool, ApiError> {
    let Some(summary) = store.get(job_id).await? else {
        return Ok(false);
    };
    if !ACTIVE_LEGACY_CRAWL_STATUSES.contains(&summary.status) {
        return Ok(false);
    }

    if matches!(
        summary.status,
        LifecycleStatus::Queued | LifecycleStatus::Pending
    ) {
        store
            .update_status(JobStatusUpdate {
                job_id,
                source_id: None,
                status: LifecycleStatus::Running,
                phase: PipelinePhase::Planning,
                stage_id: None,
                counts: None,
                current: None,
                message: Some(LEGACY_CRAWL_REMOVED_RETRY.to_string()),
                error: None,
            })
            .await?;
    }

    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Failed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: Some(LEGACY_CRAWL_REMOVED_RETRY.to_string()),
            error: Some(legacy_crawl_removed_error()),
        })
        .await?;
    Ok(true)
}

fn legacy_crawl_removed_error() -> SourceError {
    SourceError {
        code: LEGACY_CRAWL_REMOVED_CODE.to_string(),
        severity: Severity::Failed,
        message: "legacy crawl jobs were removed; re-run as a SourceRequest".to_string(),
        source_item_key: None,
        retryable: false,
        provider_id: None,
        cause: Some(LEGACY_CRAWL_REMOVED_RETRY.to_string()),
    }
}

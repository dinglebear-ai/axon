use std::error::Error;
use std::sync::Arc;

use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobId, JobKind, JobListRequest, JobRecoveryRequest,
    JobSummary, LifecycleStatus, Timestamp,
};
use axon_jobs::boundary::JobStore;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::ServiceJob;

fn parse_timestamp(value: &Timestamp) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value.0)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn service_status(status: LifecycleStatus) -> String {
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

fn request_target_fields(
    kind: JobKind,
    request_json: Option<&serde_json::Value>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<serde_json::Value>,
) {
    let Some(request_json) = request_json else {
        return (None, None, None, None);
    };

    if let Some(source_request) = request_json.get("source_request") {
        let source = source_request
            .get("source")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        return (source.clone(), None, source, None);
    }

    match kind {
        JobKind::Extract => {
            let urls_json = request_json.get("urls").cloned();
            let target = urls_json.as_ref().and_then(|urls| match urls {
                serde_json::Value::Array(items) if items.len() == 1 => {
                    items.first().and_then(|v| v.as_str()).map(String::from)
                }
                _ => None,
            });
            (None, None, target, urls_json)
        }
        _ => {
            let target = request_json
                .get("target")
                .or_else(|| request_json.get("input"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let source_type = request_json
                .get("source_type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            (None, source_type, target, None)
        }
    }
}

fn summary_to_service_job(
    summary: JobSummary,
    request_json: Option<serde_json::Value>,
) -> ServiceJob {
    let (url, source_type, target, urls_json) =
        request_target_fields(summary.kind, request_json.as_ref());
    ServiceJob {
        id: summary.job_id.0,
        status: service_status(summary.status),
        created_at: parse_timestamp(&summary.created_at),
        updated_at: parse_timestamp(&summary.updated_at),
        started_at: summary.started_at.as_ref().map(parse_timestamp),
        finished_at: summary.finished_at.as_ref().map(parse_timestamp),
        error_text: summary.last_error.as_ref().map(|e| e.message.clone()),
        url,
        source_type,
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
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(kind),
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
        jobs.push(summary_to_service_job(summary, request_json));
    }
    Ok(jobs)
}

pub(super) async fn status(
    store: &Arc<dyn JobStore>,
    kind: JobKind,
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
    if summary.kind != kind {
        return Ok(None);
    }
    let request_json = store
        .request_json(job_id)
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(Some(summary_to_service_job(summary, request_json)))
}

pub(super) async fn cancel(
    store: &Arc<dyn JobStore>,
    id: Uuid,
    reason: String,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let result = store
        .cancel(
            JobId::new(id),
            JobCancelRequest {
                reason: Some(reason),
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
    kind: JobKind,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let result = store
        .cleanup(JobCleanupRequest {
            dry_run: false,
            kind: Some(kind),
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

pub(super) async fn recover(
    store: &Arc<dyn JobStore>,
    kind: JobKind,
    stale_threshold_ms: i64,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let stale_before =
        Timestamp::from(Utc::now() - chrono::Duration::milliseconds(stale_threshold_ms.max(0)));
    let result = store
        .recover(JobRecoveryRequest {
            kind: Some(kind),
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

pub(super) async fn count(
    store: &Arc<dyn JobStore>,
    kind: JobKind,
) -> Result<i64, Box<dyn Error + Send + Sync>> {
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(kind),
            source_id: None,
            watch_id: None,
            limit: Some(1),
            cursor: None,
        })
        .await
        .map_err(|e| Box::<dyn Error + Send + Sync>::from(e.message))?;
    Ok(page.total.unwrap_or(page.items.len() as u64) as i64)
}

pub(super) async fn count_by_status(
    store: &Arc<dyn JobStore>,
    kind: JobKind,
) -> Result<
    std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
    Box<dyn Error + Send + Sync>,
> {
    let summaries = list(store, kind, 1000, 0).await?;
    let mut out = std::collections::HashMap::new();
    for summary in summaries {
        *out.entry(summary.status_enum()).or_insert(0) += 1;
    }
    Ok(out)
}

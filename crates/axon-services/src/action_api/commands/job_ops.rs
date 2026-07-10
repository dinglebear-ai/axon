use crate::context::ServiceContext;
use crate::jobs as job_svc;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{JobsRequest, JobsSubaction};
use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobClearRequest, JobEventListRequest, JobId,
    JobListRequest, JobRecoveryRequest, JobRetryMode, JobRetryRequest,
};
use axon_jobs::backend::JobKind;

use super::parse_job_id;

pub(super) async fn job_status(
    service_context: &ServiceContext,
    kind: JobKind,
    raw_id: Option<String>,
) -> Result<serde_json::Value, ClientActionError> {
    let id = parse_job_id(raw_id.as_deref())?;
    let job = job_svc::job_status(service_context, kind, id)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    Ok(serde_json::json!({ "job": job.map(|job| job.wire_json_compat()) }))
}

pub(super) async fn job_cancel(
    service_context: &ServiceContext,
    kind: JobKind,
    raw_id: Option<String>,
) -> Result<serde_json::Value, ClientActionError> {
    let id = parse_job_id(raw_id.as_deref())?;
    let canceled = job_svc::cancel_job(service_context, kind, id)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    Ok(serde_json::json!({ "job_id": id.to_string(), "canceled": canceled }))
}

pub(super) async fn job_list(
    service_context: &ServiceContext,
    kind: JobKind,
    limit: Option<i64>,
    offset: Option<usize>,
) -> Result<serde_json::Value, ClientActionError> {
    let (limit, offset) = crate::transport::job_list_pagination(limit, offset);
    let jobs = job_svc::list_jobs(service_context, kind, limit, offset)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    let jobs: Vec<_> = jobs.iter().map(|job| job.wire_json_compat()).collect();
    Ok(serde_json::json!({ "jobs": jobs, "limit": limit, "offset": offset }))
}

pub(super) async fn job_cleanup(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let deleted = job_svc::cleanup_jobs(service_context, kind)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    Ok(serde_json::json!({ "deleted": deleted }))
}

pub(super) async fn job_clear(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let deleted = job_svc::clear_jobs(service_context, kind)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    Ok(serde_json::json!({ "deleted": deleted }))
}

pub(super) async fn job_recover(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<serde_json::Value, ClientActionError> {
    let recovered = job_svc::recover_jobs(service_context, kind)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    Ok(serde_json::json!({ "recovered": recovered }))
}

pub(crate) async fn dispatch_jobs(
    service_context: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(JobsSubaction::List) {
        JobsSubaction::List => json(
            job_svc::list_unified_jobs(
                service_context,
                JobListRequest {
                    status: req.status,
                    kind: req.kind,
                    source_id: req.source_id,
                    watch_id: req.watch_id,
                    limit: req.limit,
                    cursor: req.cursor,
                },
            )
            .await?,
        ),
        JobsSubaction::Get | JobsSubaction::Status => {
            let job_id = parse_unified_job_id(req.job_id.as_deref())?;
            json(job_svc::unified_job_status(service_context, job_id).await?)
        }
        JobsSubaction::Events | JobsSubaction::Stream => {
            let job_id = parse_unified_job_id(req.job_id.as_deref())?;
            json(
                job_svc::unified_job_events(
                    service_context,
                    JobEventListRequest {
                        job_id,
                        after_sequence: req.after_sequence,
                        limit: req.limit,
                        severity: req.severity,
                        visibility: req.visibility,
                        phase: None,
                        since_sequence: req.since_sequence,
                        cursor: req.cursor,
                    },
                )
                .await?,
            )
        }
        JobsSubaction::Cancel => {
            let job_id = parse_unified_job_id(req.job_id.as_deref())?;
            json(
                job_svc::cancel_unified_job(
                    service_context,
                    job_id,
                    JobCancelRequest {
                        reason: req.reason,
                        force_after_ms: None,
                        actor: None,
                    },
                )
                .await?,
            )
        }
        JobsSubaction::Retry => {
            let job_id = parse_unified_job_id(req.job_id.as_deref())?;
            json(
                job_svc::retry_unified_job(
                    service_context,
                    job_id,
                    JobRetryRequest {
                        mode: req.retry_mode.unwrap_or(JobRetryMode::SameConfig),
                        from_phase: req.from_phase,
                        idempotency_key: req.idempotency_key,
                        overrides: req.overrides,
                    },
                )
                .await?,
            )
        }
        JobsSubaction::Recover => json(
            job_svc::recover_unified_jobs(
                service_context,
                JobRecoveryRequest {
                    kind: req.kind,
                    stale_before: req.stale_before,
                    limit: req.limit,
                    older_than_seconds: None,
                    dry_run: req.dry_run.unwrap_or(false),
                    allow_without_cutoff: false,
                },
            )
            .await
            .map_err(ClientActionError::from_job_control_error)?,
        ),
        JobsSubaction::Cleanup => json(
            job_svc::cleanup_unified_jobs(
                service_context,
                JobCleanupRequest {
                    dry_run: req.dry_run.unwrap_or(false),
                    kind: req.kind,
                    older_than: req.older_than,
                    status: req.status,
                    limit: req.limit,
                    older_than_seconds: None,
                    confirm_all_terminal: true,
                },
            )
            .await?,
        ),
        JobsSubaction::Clear => json(
            job_svc::clear_unified_jobs(
                service_context,
                JobClearRequest {
                    status: req.status,
                    confirm: req.confirm.unwrap_or(false),
                    kind: req.kind,
                    older_than: req.older_than,
                },
            )
            .await
            .map_err(ClientActionError::from_job_control_error)?,
        ),
    }
}

fn parse_unified_job_id(raw: Option<&str>) -> Result<JobId, ClientActionError> {
    let raw = raw.ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "job_id is required",
            false,
            Some("include a UUID job_id for this lifecycle action".to_string()),
        )
    })?;
    uuid::Uuid::parse_str(raw)
        .map(JobId::new)
        .map_err(|err| ClientActionError::new("invalid_request", err.to_string(), false, None))
}

fn json<T: serde::Serialize>(value: T) -> Result<serde_json::Value, ClientActionError> {
    serde_json::to_value(value)
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))
}

impl From<Box<dyn std::error::Error + Send + Sync>> for ClientActionError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        ClientActionError::new("internal", err.to_string(), true, None)
    }
}

impl ClientActionError {
    fn from_job_control_error(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        let message = err.to_string();
        if message.contains("requires")
            || message.contains("confirm=true")
            || message.contains("staleness cutoff")
        {
            Self::new("invalid_request", message, false, None)
        } else {
            Self::new("internal", message, true, None)
        }
    }
}

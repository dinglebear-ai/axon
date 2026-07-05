use super::AxonMcpServer;
use super::common::{InlineHint, invalid_params, logged_internal_error, respond_with_mode};
use crate::schema::{AxonToolResponse, JobsRequest, JobsSubaction};
use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobClearRequest, JobEventListRequest, JobId,
    JobListRequest, JobRecoveryRequest, JobRetryMode, JobRetryRequest,
};
use axon_services::context::ServiceContext;
use rmcp::ErrorData;
use uuid::Uuid;

impl AxonMcpServer {
    pub(super) async fn handle_jobs(
        &self,
        req: JobsRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let response_mode = req.response_mode;
        let subaction = req.subaction.unwrap_or(JobsSubaction::List);
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("jobs.context", e.as_ref()))?;

        let payload = match subaction {
            JobsSubaction::List => jobs_list(&ctx, req).await?,
            JobsSubaction::Get | JobsSubaction::Status => jobs_get(&ctx, req).await?,
            JobsSubaction::Events | JobsSubaction::Stream => jobs_events(&ctx, req).await?,
            JobsSubaction::Cancel => jobs_cancel(&ctx, req).await?,
            JobsSubaction::Retry => jobs_retry(&ctx, req).await?,
            JobsSubaction::Recover => jobs_recover(&ctx, req).await?,
            JobsSubaction::Cleanup => jobs_cleanup(&ctx, req).await?,
            JobsSubaction::Clear => jobs_clear(&ctx, req).await?,
        };

        respond_with_mode(
            "jobs",
            jobs_subaction_name(subaction),
            response_mode,
            "jobs",
            payload,
            InlineHint::Default,
        )
        .await
    }
}

async fn jobs_list(ctx: &ServiceContext, req: JobsRequest) -> Result<serde_json::Value, ErrorData> {
    let page = axon_services::jobs::list_unified_jobs(
        ctx,
        JobListRequest {
            status: req.status,
            kind: req.kind,
            source_id: req.source_id,
            watch_id: req.watch_id,
            limit: req.limit,
            cursor: req.cursor,
        },
    )
    .await
    .map_err(|e| logged_internal_error("jobs.list", e.as_ref()))?;
    serde_json::to_value(page).map_err(|e| logged_internal_error("jobs.list", &e))
}

async fn jobs_get(ctx: &ServiceContext, req: JobsRequest) -> Result<serde_json::Value, ErrorData> {
    let job_id = parse_unified_job_id(req.job_id.as_deref())?;
    let job = axon_services::jobs::unified_job_status(ctx, job_id)
        .await
        .map_err(|e| logged_internal_error("jobs.get", e.as_ref()))?;
    Ok(serde_json::json!({ "job": job }))
}

async fn jobs_events(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let job_id = parse_unified_job_id(req.job_id.as_deref())?;
    let page = axon_services::jobs::unified_job_events(
        ctx,
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
    .await
    .map_err(|e| logged_internal_error("jobs.events", e.as_ref()))?;
    serde_json::to_value(page).map_err(|e| logged_internal_error("jobs.events", &e))
}

async fn jobs_cancel(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let job_id = parse_unified_job_id(req.job_id.as_deref())?;
    let result = axon_services::jobs::cancel_unified_job(
        ctx,
        job_id,
        JobCancelRequest {
            reason: req.reason,
            force_after_ms: None,
        },
    )
    .await
    .map_err(|e| logged_internal_error("jobs.cancel", e.as_ref()))?;
    serde_json::to_value(result).map_err(|e| logged_internal_error("jobs.cancel", &e))
}

async fn jobs_retry(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let job_id = parse_unified_job_id(req.job_id.as_deref())?;
    let result = axon_services::jobs::retry_unified_job(
        ctx,
        job_id,
        JobRetryRequest {
            mode: req.retry_mode.unwrap_or(JobRetryMode::SameConfig),
            from_phase: req.from_phase,
            idempotency_key: req.idempotency_key,
            overrides: req.overrides,
        },
    )
    .await
    .map_err(|e| logged_internal_error("jobs.retry", e.as_ref()))?;
    serde_json::to_value(result).map_err(|e| logged_internal_error("jobs.retry", &e))
}

async fn jobs_recover(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let result = axon_services::jobs::recover_unified_jobs(
        ctx,
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
    .map_err(|e| logged_internal_error("jobs.recover", e.as_ref()))?;
    serde_json::to_value(result).map_err(|e| logged_internal_error("jobs.recover", &e))
}

async fn jobs_cleanup(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let result = axon_services::jobs::cleanup_unified_jobs(
        ctx,
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
    .await
    .map_err(|e| logged_internal_error("jobs.cleanup", e.as_ref()))?;
    serde_json::to_value(result).map_err(|e| logged_internal_error("jobs.cleanup", &e))
}

async fn jobs_clear(
    ctx: &ServiceContext,
    req: JobsRequest,
) -> Result<serde_json::Value, ErrorData> {
    let result = axon_services::jobs::clear_unified_jobs(
        ctx,
        JobClearRequest {
            status: req.status,
            confirm: req.confirm.unwrap_or(false),
            kind: req.kind,
            older_than: req.older_than,
        },
    )
    .await
    .map_err(|e| logged_internal_error("jobs.clear", e.as_ref()))?;
    serde_json::to_value(result).map_err(|e| logged_internal_error("jobs.clear", &e))
}

fn parse_unified_job_id(raw: Option<&str>) -> Result<JobId, ErrorData> {
    let raw = raw.ok_or_else(|| invalid_params("job_id is required"))?;
    Uuid::parse_str(raw)
        .map(JobId::new)
        .map_err(|e| invalid_params(format!("invalid job_id: {e}")))
}

fn jobs_subaction_name(subaction: JobsSubaction) -> &'static str {
    match subaction {
        JobsSubaction::List => "list",
        JobsSubaction::Get => "get",
        JobsSubaction::Status => "status",
        JobsSubaction::Events => "events",
        JobsSubaction::Stream => "stream",
        JobsSubaction::Cancel => "cancel",
        JobsSubaction::Retry => "retry",
        JobsSubaction::Recover => "recover",
        JobsSubaction::Cleanup => "cleanup",
        JobsSubaction::Clear => "clear",
    }
}

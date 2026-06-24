use crate::context::ServiceContext;
use crate::jobs as job_svc;
use crate::types::ClientActionError;
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

use axon_api::job_progress::{JobFamily, JobProgress};
use axon_jobs::backend::JobKind;
use axon_services as services;
use axon_services::context::ServiceContext;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::super::error::HttpError;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct JobListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Typed job-status envelope so the `{ job, progress }` wire shape is a
/// registered OpenAPI schema (and thus reflected into the generated palette/
/// android clients) instead of an opaque `serde_json::Value`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct JobStatusResponse {
    /// Raw job record in the wire-compat shape (`status`, `result_json`,
    /// timestamps, …). Still `Value` because the per-family job payloads are
    /// heterogeneous; `progress` is the typed, cross-family projection of it.
    pub job: serde_json::Value,
    /// Server-derived, transport-neutral progress for the generic async
    /// families (embed/extract/ingest). `None` for crawl, which carries a
    /// richer client-side snapshot rather than the generic shape.
    pub progress: Option<JobProgress>,
}

#[derive(Clone)]
pub(crate) struct JobLifecycleState {
    service_context: Arc<ServiceContext>,
    kind: JobKind,
}

pub(crate) fn job_lifecycle_router<S>(
    service_context: Arc<ServiceContext>,
    kind: JobKind,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = JobLifecycleState {
        service_context,
        kind,
    };
    Router::new()
        .route("/", get(list_jobs).delete(clear_jobs))
        .route("/{id}", get(job_status))
        .route("/{id}/cancel", post(cancel_job))
        .route("/cleanup", post(cleanup_jobs))
        .route("/recover", post(recover_jobs))
        .layer(Extension(state))
}

#[utoipa::path(
    get,
    path = "/v1/crawl",
    params(JobListQuery),
    responses((status = 200, description = "Crawl jobs", body = serde_json::Value)),
    tag = "jobs"
)]
pub(crate) async fn list_jobs(
    Extension(state): Extension<JobLifecycleState>,
    Query(query): Query<JobListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let (limit, offset) =
        services::transport::job_list_pagination_signed(query.limit, query.offset);
    let jobs = services::jobs::list_jobs(&state.service_context, state.kind, limit, offset)
        .await
        .map_err(HttpError::from_box)?;
    let jobs: Vec<_> = jobs.iter().map(|job| job.wire_json_compat()).collect();
    Ok(Json(json!({
        "jobs": jobs,
        "limit": limit,
        "offset": offset,
    })))
}

#[utoipa::path(
    get,
    path = "/v1/crawl/{id}",
    params(("id" = uuid::Uuid, Path, description = "Crawl job ID")),
    responses((status = 200, description = "Crawl job status", body = JobStatusResponse), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(crate) async fn job_status(
    Extension(state): Extension<JobLifecycleState>,
    Path(id): Path<Uuid>,
) -> Result<Json<JobStatusResponse>, HttpError> {
    let job = services::jobs::job_status(&state.service_context, state.kind, id)
        .await
        .map_err(HttpError::from_box)?;
    let Some(job) = job else {
        return Err(HttpError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            format!("job not found: {id}"),
        ));
    };
    // Canonical, server-derived progress for the generic async families so the
    // palette/android/CLI consume it instead of re-deriving phase/percent/metrics.
    // Crawl keeps its richer client-side snapshot.
    let progress = job_family(state.kind).map(|family| JobProgress::from_service_job(family, &job));
    Ok(Json(JobStatusResponse {
        job: job.wire_json_compat(),
        progress,
    }))
}

/// Map a job-runtime `JobKind` to the generic progress family, or `None` for
/// crawl (which has a bespoke client-side snapshot, not the generic shape).
fn job_family(kind: JobKind) -> Option<JobFamily> {
    match kind {
        JobKind::Embed => Some(JobFamily::Embed),
        JobKind::Extract => Some(JobFamily::Extract),
        JobKind::Ingest => Some(JobFamily::Ingest),
        JobKind::Crawl => None,
    }
}

#[utoipa::path(
    post,
    path = "/v1/crawl/{id}/cancel",
    params(("id" = uuid::Uuid, Path, description = "Crawl job ID")),
    responses((status = 200, description = "Crawl cancellation result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(crate) async fn cancel_job(
    Extension(state): Extension<JobLifecycleState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let canceled = services::jobs::cancel_job(&state.service_context, state.kind, id)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({
        "job_id": id,
        "canceled": canceled,
    })))
}

#[utoipa::path(
    post,
    path = "/v1/crawl/cleanup",
    responses((status = 200, description = "Crawl cleanup result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(crate) async fn cleanup_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let deleted = services::jobs::cleanup_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "deleted": deleted })))
}

#[utoipa::path(
    delete,
    path = "/v1/crawl",
    responses((status = 200, description = "Crawl clear result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(crate) async fn clear_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let deleted = services::jobs::clear_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "deleted": deleted })))
}

#[utoipa::path(
    post,
    path = "/v1/crawl/recover",
    responses((status = 200, description = "Crawl recovery result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(crate) async fn recover_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let recovered = services::jobs::recover_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "recovered": recovered })))
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;

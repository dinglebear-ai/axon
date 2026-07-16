use axon_api::source::{
    JobCancelRequest, JobCleanupRequest, JobClearRequest, JobEventListRequest, JobKind,
    JobListRequest, JobRecoveryRequest, JobRetryRequest, LifecycleStatus, Severity, Visibility,
};
use axon_services as services;
use axon_services::context::ServiceContext;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::super::error::HttpError;

fn json_value<T: Serialize>(value: T) -> Result<serde_json::Value, HttpError> {
    serde_json::to_value(value)
        .map_err(|error| HttpError::from_error(&std::io::Error::other(error.to_string())))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UnifiedJobListQuery {
    status: Option<LifecycleStatus>,
    kind: Option<JobKind>,
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UnifiedJobEventsQuery {
    after_sequence: Option<u64>,
    since_sequence: Option<u64>,
    limit: Option<u32>,
    severity: Option<Severity>,
    visibility: Option<Visibility>,
    cursor: Option<String>,
}

#[derive(Clone)]
pub(crate) struct UnifiedJobsState {
    pub(super) service_context: Arc<ServiceContext>,
}

pub(crate) fn unified_jobs_read_router<S>(service_context: Arc<ServiceContext>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list_unified_jobs))
        .route("/{id}", get(unified_job_status))
        .route("/{id}/events", get(unified_job_events))
        .route("/{id}/stream", get(super::jobs_stream::unified_job_stream))
        .route("/{id}/artifacts", get(unified_job_artifacts))
        .layer(Extension(UnifiedJobsState { service_context }))
}

pub(crate) fn unified_jobs_write_router<S>(service_context: Arc<ServiceContext>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/{id}/cancel", post(cancel_unified_job))
        .route("/{id}/retry", post(retry_unified_job))
        .layer(Extension(UnifiedJobsState { service_context }))
}

pub(crate) fn unified_jobs_admin_router<S>(service_context: Arc<ServiceContext>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", axum::routing::delete(clear_unified_jobs))
        .route("/recover", post(recover_unified_jobs))
        .route("/cleanup", post(cleanup_unified_jobs))
        .layer(Extension(UnifiedJobsState { service_context }))
}

#[utoipa::path(
    get,
    path = "/v1/jobs",
    params(UnifiedJobListQuery),
    responses((status = 200, description = "Unified jobs", body = axon_api::source::JobSummaryPage)),
    tag = "jobs"
)]
pub(crate) async fn list_unified_jobs(
    Extension(state): Extension<UnifiedJobsState>,
    Query(query): Query<UnifiedJobListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let page = services::jobs::list_unified_jobs(
        &state.service_context,
        JobListRequest {
            status: query.status,
            kind: query.kind,
            source_id: None,
            watch_id: None,
            limit: query.limit,
            cursor: query.cursor,
        },
    )
    .await
    .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(page)?))
}

#[utoipa::path(
    get,
    path = "/v1/jobs/{id}",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID")),
    responses((status = 200, description = "Unified job status", body = axon_api::source::JobSummary), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(crate) async fn unified_job_status(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let job_id = axon_api::source::JobId::new(id);
    let Some(job) = services::jobs::unified_job_status(&state.service_context, job_id)
        .await
        .map_err(HttpError::from_box_send_sync)?
    else {
        return Err(HttpError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            format!("job not found: {id}"),
        ));
    };
    Ok(Json(json_value(job)?))
}

#[utoipa::path(
    get,
    path = "/v1/jobs/{id}/events",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID"), UnifiedJobEventsQuery),
    responses((status = 200, description = "Unified job event page", body = axon_api::source::JobEventPage)),
    tag = "jobs"
)]
pub(crate) async fn unified_job_events(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
    Query(query): Query<UnifiedJobEventsQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let page = services::jobs::unified_job_events(
        &state.service_context,
        JobEventListRequest {
            job_id: axon_api::source::JobId::new(id),
            after_sequence: query.after_sequence,
            limit: query.limit,
            severity: query.severity,
            visibility: query.visibility,
            phase: None,
            since_sequence: query.since_sequence,
            cursor: query.cursor,
        },
    )
    .await
    .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(page)?))
}

#[utoipa::path(
    get,
    path = "/v1/jobs/{id}/artifacts",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID")),
    responses((status = 200, description = "Unified job artifacts", body = axon_api::source::JobArtifactListResult)),
    tag = "jobs"
)]
pub(crate) async fn unified_job_artifacts(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::unified_job_artifacts(
        &state.service_context,
        axon_api::source::JobArtifactListRequest {
            job_id: axon_api::source::JobId::new(id),
            kind: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[utoipa::path(
    post,
    path = "/v1/jobs/{id}/cancel",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID")),
    request_body = JobCancelRequest,
    responses((status = 200, description = "Unified job cancellation", body = axon_api::source::JobCancelResult)),
    tag = "jobs"
)]
pub(crate) async fn cancel_unified_job(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
    Json(request): Json<JobCancelRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::cancel_unified_job(
        &state.service_context,
        axon_api::source::JobId::new(id),
        request,
    )
    .await
    .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[utoipa::path(
    post,
    path = "/v1/jobs/{id}/retry",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID")),
    request_body = JobRetryRequest,
    responses((status = 200, description = "Unified job retry", body = axon_api::source::JobRetryResult)),
    tag = "jobs"
)]
pub(crate) async fn retry_unified_job(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
    Json(request): Json<JobRetryRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::retry_unified_job(
        &state.service_context,
        axon_api::source::JobId::new(id),
        request,
    )
    .await
    .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[utoipa::path(
    post,
    path = "/v1/jobs/recover",
    request_body = JobRecoveryRequest,
    responses((status = 200, description = "Unified job recovery", body = axon_api::source::JobRecoveryResult)),
    tag = "jobs"
)]
pub(crate) async fn recover_unified_jobs(
    Extension(state): Extension<UnifiedJobsState>,
    Json(request): Json<JobRecoveryRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::recover_unified_jobs(&state.service_context, request)
        .await
        .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[utoipa::path(
    post,
    path = "/v1/jobs/cleanup",
    request_body = JobCleanupRequest,
    responses((status = 200, description = "Unified job cleanup", body = axon_api::source::JobCleanupResult)),
    tag = "jobs"
)]
pub(crate) async fn cleanup_unified_jobs(
    Extension(state): Extension<UnifiedJobsState>,
    Json(request): Json<JobCleanupRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::cleanup_unified_jobs(&state.service_context, request)
        .await
        .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[utoipa::path(
    delete,
    path = "/v1/jobs",
    request_body = JobClearRequest,
    responses((status = 200, description = "Unified job clear", body = axon_api::source::JobClearResult)),
    tag = "jobs"
)]
pub(crate) async fn clear_unified_jobs(
    Extension(state): Extension<UnifiedJobsState>,
    Json(request): Json<JobClearRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let result = services::jobs::clear_unified_jobs(&state.service_context, request)
        .await
        .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(json_value(result)?))
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;

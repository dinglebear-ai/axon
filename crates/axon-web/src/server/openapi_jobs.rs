#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct JobListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/extract",
    params(JobListQuery),
    responses((status = 200, description = "Extract jobs", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn list_extract_jobs() {}

#[utoipa::path(
    get,
    path = "/v1/extract/{id}",
    params(("id" = uuid::Uuid, Path, description = "Extract job ID")),
    responses((status = 200, description = "Extract job status", body = crate::server::handlers::jobs::JobStatusResponse), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(super) async fn extract_job_status() {}

#[utoipa::path(
    post,
    path = "/v1/extract/{id}/cancel",
    params(("id" = uuid::Uuid, Path, description = "Extract job ID")),
    responses((status = 200, description = "Extract cancellation result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cancel_extract_job() {}

#[utoipa::path(
    post,
    path = "/v1/extract/cleanup",
    responses((status = 200, description = "Extract cleanup result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cleanup_extract_jobs() {}

#[utoipa::path(
    delete,
    path = "/v1/extract",
    responses((status = 200, description = "Extract clear result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn clear_extract_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/extract/recover",
    responses((status = 200, description = "Extract recovery result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn recover_extract_jobs() {}

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
    path = "/v1/embed",
    params(JobListQuery),
    responses((status = 200, description = "Embed jobs", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn list_embed_jobs() {}

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
    path = "/v1/ingest",
    params(JobListQuery),
    responses((status = 200, description = "Ingest jobs", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn list_ingest_jobs() {}

#[utoipa::path(
    get,
    path = "/v1/embed/{id}",
    params(("id" = uuid::Uuid, Path, description = "Embed job ID")),
    responses((status = 200, description = "Embed job status", body = serde_json::Value), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(super) async fn embed_job_status() {}

#[utoipa::path(
    get,
    path = "/v1/extract/{id}",
    params(("id" = uuid::Uuid, Path, description = "Extract job ID")),
    responses((status = 200, description = "Extract job status", body = serde_json::Value), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(super) async fn extract_job_status() {}

#[utoipa::path(
    get,
    path = "/v1/ingest/{id}",
    params(("id" = uuid::Uuid, Path, description = "Ingest job ID")),
    responses((status = 200, description = "Ingest job status", body = serde_json::Value), (status = 404, description = "Job not found", body = crate::server::error::ErrorBody)),
    tag = "jobs"
)]
pub(super) async fn ingest_job_status() {}

#[utoipa::path(
    post,
    path = "/v1/embed/{id}/cancel",
    params(("id" = uuid::Uuid, Path, description = "Embed job ID")),
    responses((status = 200, description = "Embed cancellation result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cancel_embed_job() {}

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
    path = "/v1/ingest/{id}/cancel",
    params(("id" = uuid::Uuid, Path, description = "Ingest job ID")),
    responses((status = 200, description = "Ingest cancellation result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cancel_ingest_job() {}

#[utoipa::path(
    post,
    path = "/v1/embed/cleanup",
    responses((status = 200, description = "Embed cleanup result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cleanup_embed_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/extract/cleanup",
    responses((status = 200, description = "Extract cleanup result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cleanup_extract_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/ingest/cleanup",
    responses((status = 200, description = "Ingest cleanup result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn cleanup_ingest_jobs() {}

#[utoipa::path(
    delete,
    path = "/v1/embed",
    responses((status = 200, description = "Embed clear result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn clear_embed_jobs() {}

#[utoipa::path(
    delete,
    path = "/v1/extract",
    responses((status = 200, description = "Extract clear result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn clear_extract_jobs() {}

#[utoipa::path(
    delete,
    path = "/v1/ingest",
    responses((status = 200, description = "Ingest clear result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn clear_ingest_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/embed/recover",
    responses((status = 200, description = "Embed recovery result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn recover_embed_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/extract/recover",
    responses((status = 200, description = "Extract recovery result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn recover_extract_jobs() {}

#[utoipa::path(
    post,
    path = "/v1/ingest/recover",
    responses((status = 200, description = "Ingest recovery result", body = serde_json::Value)),
    tag = "jobs"
)]
pub(super) async fn recover_ingest_jobs() {}

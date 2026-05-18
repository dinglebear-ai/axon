use crate::jobs::backend::JobKind;
use crate::services;
use crate::services::context::ServiceContext;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::super::error::HttpError;

#[derive(Debug, Deserialize)]
struct JobListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Clone)]
struct JobLifecycleState {
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

async fn list_jobs(
    Extension(state): Extension<JobLifecycleState>,
    Query(query): Query<JobListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let jobs = services::jobs::list_jobs(&state.service_context, state.kind, limit, offset)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({
        "jobs": jobs,
        "limit": limit,
        "offset": offset,
    })))
}

async fn job_status(
    Extension(state): Extension<JobLifecycleState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, HttpError> {
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
    Ok(Json(json!({ "job": job })))
}

async fn cancel_job(
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

async fn cleanup_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let deleted = services::jobs::cleanup_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn clear_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let deleted = services::jobs::clear_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn recover_jobs(
    Extension(state): Extension<JobLifecycleState>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let recovered = services::jobs::recover_jobs(&state.service_context, state.kind)
        .await
        .map_err(HttpError::from_box)?;
    Ok(Json(json!({ "recovered": recovered })))
}

use super::super::error::{map_service_error, rest_error};
use super::super::state::RestState;
use crate::services::context::ServiceContext;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn missing_field(field: &'static str) -> Response {
    rest_error(
        StatusCode::BAD_REQUEST,
        "bad_request",
        format!("{field} is required"),
    )
}

pub(super) fn not_found(kind: &'static str, id: Uuid) -> Response {
    rest_error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{kind} job {id} not found"),
    )
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
pub(super) async fn ctx_only(state: &RestState) -> Result<Arc<ServiceContext>, Response> {
    state
        .service_context()
        .await
        .map_err(|err| map_service_error(&*err))
}

#[allow(clippy::result_large_err)] // Err is an Axum Response we just return as-is.
pub(super) async fn ctx_and_job_id(
    state: &RestState,
    id: &str,
) -> Result<(Arc<ServiceContext>, Uuid), Response> {
    let job_id = Uuid::parse_str(id).map_err(|_| {
        rest_error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid job id: {id}"),
        )
    })?;
    let ctx = ctx_only(state).await?;
    Ok((ctx, job_id))
}

pub(super) fn cancel_response(canceled: bool) -> Response {
    Json(serde_json::json!({ "canceled": canceled })).into_response()
}

pub(super) fn count_response(action: &'static str, count: u64) -> Response {
    Json(serde_json::json!({ action: count })).into_response()
}

pub(super) fn validate_urls(urls: &[String]) -> Result<(), String> {
    for url in urls {
        crate::core::http::validate_url(url).map_err(|e| format!("{url}: {e}"))?;
    }
    Ok(())
}

pub(super) fn validate_embed_input(
    cfg: &crate::core::config::Config,
    input: &str,
) -> Result<String, String> {
    crate::services::embed::validate_server_embed_input_with_config(cfg, input)
        .map_err(|err| err.to_string())
}

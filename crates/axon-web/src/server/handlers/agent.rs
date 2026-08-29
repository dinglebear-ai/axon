use axon_core::config::Config;
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;

use super::super::error::HttpError;

#[derive(Debug, Deserialize)]
pub struct EventCursor {
    #[serde(default)]
    after: u64,
}

#[utoipa::path(get, path = "/v1/agent/turns/{id}", params(("id" = String, Path)), responses((status = 200, body = axon_api::agent::AgentTurnResult), (status = 404, body = crate::server::error::ErrorBody)), tag = "rag")]
pub async fn v1_agent_status(
    Extension(cfg): Extension<Arc<Config>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match axon_services::agent_runtime::status(&cfg, &id) {
        Ok(result) => Json(result).into_response(),
        Err(error) => HttpError::new(
            StatusCode::NOT_FOUND,
            "agent_turn_not_found",
            error.to_string(),
        )
        .into_response(),
    }
}

#[utoipa::path(get, path = "/v1/agent/turns/{id}/events", params(("id" = String, Path), ("after" = Option<u64>, Query)), responses((status = 200, description = "Ordered durable agent events"), (status = 404, body = crate::server::error::ErrorBody)), tag = "rag")]
pub async fn v1_agent_events(
    Extension(cfg): Extension<Arc<Config>>,
    Path(id): Path<String>,
    Query(cursor): Query<EventCursor>,
) -> impl IntoResponse {
    match axon_services::agent_runtime::events(&cfg, &id, cursor.after) {
        Ok(events) => Json(serde_json::json!({ "items": events })).into_response(),
        Err(error) => HttpError::new(
            StatusCode::NOT_FOUND,
            "agent_turn_not_found",
            error.to_string(),
        )
        .into_response(),
    }
}

#[utoipa::path(post, path = "/v1/agent/turns/{id}/cancel", params(("id" = String, Path)), responses((status = 200, body = axon_api::agent::AgentTurnResult), (status = 502, body = crate::server::error::ErrorBody)), tag = "rag")]
pub async fn v1_agent_cancel(
    Extension(cfg): Extension<Arc<Config>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match axon_services::agent_runtime::cancel(&cfg, &id).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => HttpError::new(
            StatusCode::BAD_GATEWAY,
            "agent_cancel_failed",
            error.to_string(),
        )
        .into_response(),
    }
}

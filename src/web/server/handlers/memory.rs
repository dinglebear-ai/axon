use crate::core::config::Config;
use crate::services;
use crate::services::client_contract::RestMemoryRequest as MemoryRequest;
use crate::services::types::ClientActionError;
use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use super::super::error::HttpError;

type WebState = (super::super::state::AppState, Arc<Config>);

#[utoipa::path(
    post,
    path = "/v1/memory",
    request_body = MemoryRequest,
    responses(
        (status = 200, description = "Persistent memory result", body = serde_json::Value),
        (status = 400, description = "Invalid memory request", body = crate::web::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::web::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn memory(
    State((state, _cfg)): State<WebState>,
    Json(req): Json<MemoryRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    services::memory::dispatch(&state.service_context, req.into())
        .await
        .map(Json)
        .map_err(memory_error)
}

fn memory_error(err: ClientActionError) -> HttpError {
    let message = match err.hint {
        Some(hint) => format!("{}: {hint}", err.message),
        None => err.message,
    };
    let lower = message.to_lowercase();
    if err.retryable || err.kind == "internal" {
        HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    } else if lower.contains("qdrant")
        || lower.contains("tei")
        || lower.contains("connection refused")
    {
        HttpError::new(StatusCode::BAD_GATEWAY, "upstream_unavailable", message)
    } else {
        HttpError::bad_request(message)
    }
}

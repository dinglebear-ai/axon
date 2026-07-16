//! Typed `/v1/memories/*` handlers.

use axon_services::types::ClientActionError;
use axum::http::StatusCode;

use super::super::error::HttpError;
use super::super::json::Json;

#[path = "memory_routes.rs"]
mod routes;
pub(crate) use routes::*;

pub(super) fn memory_error(err: ClientActionError) -> HttpError {
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

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;

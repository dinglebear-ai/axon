//! `POST /v1/memory` — the deprecated single-route memory passthrough, kept
//! functional (marked `Deprecation`) alongside the per-verb `/v1/memories`
//! routes in [`routes`]. See `docs/pipeline-unification/plans/
//! 2026-07-08-rest-memory-surface.md` Task 2/4 for the deprecation-not-removal
//! rationale — the desktop palette app and other external clients may still
//! depend on this shape until they migrate.

use axon_core::config::Config;
use axon_services as services;
use axon_services::client_contract::RestMemoryRequest as MemoryRequest;
use axon_services::types::ClientActionError;
use axum::{
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::json::Json;

#[path = "memory_routes.rs"]
mod routes;
pub(crate) use routes::*;

type WebState = (super::super::state::AppState, Arc<Config>);

/// RFC 8594 deprecation signal for the legacy passthrough. No sunset date is
/// set yet — removal is tracked by a filed follow-up (Task 4), not a fixed
/// date, so clients get a durable warning without a hard cutoff commitment.
const DEPRECATION_HEADER_NAME: header::HeaderName = header::HeaderName::from_static("deprecation");
const DEPRECATION_HEADER_VALUE: &str = "true";
const DEPRECATION_LINK: &str = "</v1/memories>; rel=\"successor-version\"";

#[utoipa::path(
    post,
    path = "/v1/memory",
    request_body = MemoryRequest,
    responses(
        (status = 200, description = "Persistent memory result (deprecated — use /v1/memories)", body = serde_json::Value),
        (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn memory(
    State((state, _cfg)): State<WebState>,
    Json(req): Json<MemoryRequest>,
) -> Response {
    tracing::warn!("POST /v1/memory is deprecated; migrate to the per-verb /v1/memories routes");
    // `RestMemorySubaction` (the type accepted by this deprecated passthrough)
    // has no `Import` variant, so `MemoryAuthz::anonymous()` is safe here —
    // none of the reachable subactions consult it. Import/export are separate
    // typed routes (`import_memories`/`export_memories`).
    let mut response = services::memory::dispatch(
        &state.service_context,
        req.into(),
        &services::memory::MemoryAuthz::anonymous(),
    )
    .await
    .map(Json)
    .map_err(memory_error)
    .into_response();
    // Set on both success and error responses — deprecation is a route-level
    // property of `POST /v1/memory` itself, not conditional on the payload
    // being valid.
    response.headers_mut().insert(
        DEPRECATION_HEADER_NAME,
        HeaderValue::from_static(DEPRECATION_HEADER_VALUE),
    );
    response
        .headers_mut()
        .insert(header::LINK, HeaderValue::from_static(DEPRECATION_LINK));
    response
}

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

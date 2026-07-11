//! Ledger-backed read routes and the read-only route resolver preview for the
//! unified source pipeline (#298 / WS-G resource tier).
//!
//! `GET /v1/sources/{source_id}` calls the real [`axon_ledger::store::LedgerStore`]
//! directly (a domain crate's public entry, per
//! `docs/architecture/crate-ownership.md`) via
//! [`ServiceContext::target_local_source_runtime`]. That runtime is only
//! constructed when Qdrant + TEI are configured on a worker-bearing context
//! (`serve`/`mcp`); when it is absent this route reports `503`, not a fake
//! `404`.
//!
//! `POST /v1/resolve` wraps [`axon_services::source::routing::resolve_source_route`],
//! which is already a pure/read-only resolver (no mutation, no job creation) —
//! it classifies + routes a [`SourceRequest`] and returns the resulting
//! [`RoutePlan`] without acquiring anything.
//!
//! `SourceService::get`/`list`/`items`/`generations` remain `not_implemented`
//! in production (see `crates/axon-services/src/service_traits/source_service.rs`)
//! because no backing free function exists for them yet; those sub-resources
//! (`/items`, `/generations`, `/documents`, `PATCH`, `DELETE`, refresh-by-id)
//! are intentionally NOT wired here — see the WS-G followups list.

use axon_api::ApiError;
use axon_api::source::{RoutePlan, SourceId, SourceRequest, SourceSummary};
use axon_error::ErrorStage;
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;

use super::super::error::HttpError;
use super::super::json::Json;
use super::super::state::AppState;

type WebState = (AppState, Arc<axon_core::config::Config>);

#[utoipa::path(
    get,
    path = "/v1/sources/{source_id}",
    params(("source_id" = String, Path, description = "Ledger source id")),
    responses(
        (status = 200, description = "Source detail", body = SourceSummary),
        (status = 404, description = "Source not found in the ledger", body = crate::server::error::ErrorBody),
        (status = 503, description = "Source ledger not configured on this deployment", body = crate::server::error::ErrorBody)
    ),
    tag = "sources"
)]
pub(crate) async fn get_source(
    State((state, _cfg)): State<WebState>,
    Path(source_id): Path<String>,
) -> Result<Json<SourceSummary>, HttpError> {
    let runtime = ledger_runtime(state.service_context.target_local_source_runtime())?;
    let summary = runtime
        .ledger
        .get_source(SourceId::new(source_id.clone()))
        .await
        .map_err(HttpError::from_api_error)?
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("source {source_id} not found"),
            )
        })?;
    Ok(Json(summary))
}

fn ledger_runtime(
    runtime: Option<&axon_services::context::TargetLocalSourceRuntime>,
) -> Result<&axon_services::context::TargetLocalSourceRuntime, HttpError> {
    runtime.ok_or_else(|| {
        HttpError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "upstream_unavailable",
            "source ledger is not configured on this deployment (requires QDRANT_URL + TEI_URL on a worker-bearing context)",
        )
    })
}

#[utoipa::path(
    post,
    path = "/v1/resolve",
    request_body = SourceRequest,
    responses(
        (status = 200, description = "Resolved source route plan (no mutation)", body = RoutePlan),
        (status = 400, description = "Invalid source request", body = crate::server::error::ErrorBody),
        (status = 422, description = "Source could not be resolved", body = crate::server::error::ErrorBody)
    ),
    tag = "sources"
)]
pub(crate) async fn resolve_source(
    Json(request): Json<SourceRequest>,
) -> Result<Json<RoutePlan>, HttpError> {
    if request.source.trim().is_empty() {
        return Err(HttpError::from_api_error(
            ApiError::new(
                "route.validation.missing_field",
                ErrorStage::Validation,
                "source is required",
            )
            .with_context("field", "source"),
        ));
    }
    let routed = axon_services::source::routing::resolve_source_route(&request)
        .map_err(HttpError::from_api_error)?;
    Ok(Json(routed.route))
}

#[cfg(test)]
#[path = "sources_resource_tests.rs"]
mod tests;

//! `GET /v1/providers` and `GET /v1/providers/{provider}` — a REST
//! resource-tier projection over the real per-service health/capability data
//! `services::system::doctor` already collects (Qdrant/TEI/Chrome/LLM
//! reachability probes in `axon_core::health::build_doctor_report`).
//!
//! This does not re-probe anything: it calls the same `doctor()` service the
//! `/v1/doctor` route already uses and reshapes its `services` object into a
//! stable per-provider list/detail shape. There is no separate
//! provider-capability service to call yet (see the WS-G followups list), so
//! this is the only real backing data available for the contract's
//! `/v1/providers*` routes.

use axon_services as services;
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use super::super::error::HttpError;
use super::super::json::Json;

type WebState = (
    super::super::state::AppState,
    Arc<axon_core::config::Config>,
);

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ProviderSummary {
    pub id: String,
    /// `true` when the doctor probe for this provider reported healthy.
    pub ok: bool,
    /// Raw per-provider diagnostics payload from the doctor report.
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct ProviderListResponse {
    pub providers: Vec<ProviderSummary>,
}

fn provider_summaries(doctor_payload: &serde_json::Value) -> Vec<ProviderSummary> {
    let Some(services_map) = doctor_payload.get("services").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut providers: Vec<ProviderSummary> = services_map
        .iter()
        .map(|(id, detail)| ProviderSummary {
            id: id.clone(),
            ok: detail
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            detail: detail.clone(),
        })
        .collect();
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    providers
}

#[utoipa::path(
    get,
    path = "/v1/providers",
    responses(
        (status = 200, description = "Provider capability/health list", body = ProviderListResponse),
        (status = 502, description = "Health check failed", body = crate::server::error::ErrorBody)
    ),
    tag = "providers"
)]
pub(crate) async fn list_providers(
    State((state, _cfg)): State<WebState>,
) -> Result<Json<ProviderListResponse>, HttpError> {
    let doctor = services::system::doctor(&state.service_context)
        .await
        .map_err(HttpError::from_box_send_sync)?;
    Ok(Json(ProviderListResponse {
        providers: provider_summaries(&doctor.payload),
    }))
}

#[utoipa::path(
    get,
    path = "/v1/providers/{provider}",
    params(("provider" = String, Path, description = "Provider id, e.g. qdrant/tei/chrome/llm")),
    responses(
        (status = 200, description = "One provider capability/health report", body = ProviderSummary),
        (status = 404, description = "Unknown provider id", body = crate::server::error::ErrorBody),
        (status = 502, description = "Health check failed", body = crate::server::error::ErrorBody)
    ),
    tag = "providers"
)]
pub(crate) async fn get_provider(
    State((state, _cfg)): State<WebState>,
    Path(provider): Path<String>,
) -> Result<Json<ProviderSummary>, HttpError> {
    let doctor = services::system::doctor(&state.service_context)
        .await
        .map_err(HttpError::from_box_send_sync)?;
    provider_summaries(&doctor.payload)
        .into_iter()
        .find(|p| p.id == provider)
        .map(Json)
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("provider {provider} not found"),
            )
        })
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;

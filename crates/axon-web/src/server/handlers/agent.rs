use axon_core::config::Config;
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use lab_auth::AuthContext;
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
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match axon_services::agent_runtime::status(&cfg, &id, &auth.sub).await {
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
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Query(cursor): Query<EventCursor>,
) -> impl IntoResponse {
    match axon_services::agent_runtime::events(&cfg, &id, &auth.sub, cursor.after).await {
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
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !cancel_scope_allowed(&auth.scopes) {
        return HttpError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "requires scope: axon:write",
        )
        .into_response();
    }
    match axon_services::agent_runtime::cancel(&cfg, &id, &auth.sub).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => HttpError::new(
            StatusCode::BAD_GATEWAY,
            "agent_cancel_failed",
            error.to_string(),
        )
        .into_response(),
    }
}

#[utoipa::path(post, path = "/v1/agent/turns/{id}/resume", request_body = axon_api::agent::AgentResumeRequest, params(("id" = String, Path)), responses((status = 200, body = axon_api::agent::AgentTurnResult)), tag = "rag")]
pub async fn v1_agent_resume(
    Extension(cfg): Extension<Arc<Config>>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(request): Json<axon_api::agent::AgentResumeRequest>,
) -> impl IntoResponse {
    if !resume_scope_allowed(&auth.scopes) {
        return HttpError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "requires explicit scope: axon:write",
        )
        .into_response();
    }
    let completion = axon_services::agent_runtime::configured_completion((*cfg).clone());
    let owner = axon_services::agent_runtime::AgentTurnOwner {
        principal: auth.sub,
        profile_id: cfg.labby_integration_id.clone().unwrap_or_default(),
    };
    match axon_services::agent_runtime::resume(
        &cfg,
        &id,
        owner,
        request.approval_tokens,
        completion,
    )
    .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => HttpError::new(
            StatusCode::CONFLICT,
            "agent_resume_failed",
            error.to_string(),
        )
        .into_response(),
    }
}

fn resume_scope_allowed(scopes: &[String]) -> bool {
    axon_authz::has_explicit_scope(scopes, axon_authz::AXON_WRITE_SCOPE)
}

fn cancel_scope_allowed(scopes: &[String]) -> bool {
    axon_authz::has_explicit_scope(scopes, axon_authz::AXON_WRITE_SCOPE)
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;

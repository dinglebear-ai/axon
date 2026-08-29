use axon_services::codex_control::{
    ControlAction, EventCursor, MutationAction, OperationIntent, OperationPhase, RecordedEvent,
};
use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lab_auth::AuthContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use utoipa::ToSchema;

use super::super::error::HttpError;
use super::super::json::Json;
use super::super::state::AppState;

type WebState = (AppState, Arc<axon_core::config::Config>);

fn service(
    state: &AppState,
) -> Result<Arc<axon_services::codex_control::CodexControlService>, HttpError> {
    match &state.codex_control {
        Ok(Some(service)) => Ok(Arc::clone(service)),
        Ok(None) => Err(HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Codex control is disabled",
        )),
        Err(error) => Err(HttpError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "upstream_unavailable",
            error.clone(),
        )),
    }
}

pub async fn snapshot(
    State((state, _)): State<WebState>,
) -> Result<Json<axon_services::codex_control::CodexControlSnapshot>, HttpError> {
    service(&state)?
        .snapshot()
        .await
        .map(Json)
        .map_err(upstream)
}

#[derive(Deserialize)]
pub struct EventsQuery {
    boot_id: Option<u64>,
    after: Option<u64>,
    limit: Option<usize>,
}

pub async fn events(
    State((state, _)): State<WebState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<RecordedEvent>>, HttpError> {
    let cursor = match (query.boot_id, query.after) {
        (Some(boot_id), Some(sequence)) => Some(EventCursor { boot_id, sequence }),
        _ => None,
    };
    service(&state)?
        .events_after(cursor, query.limit.unwrap_or(100))
        .await
        .map(Json)
        .map_err(upstream)
}

pub async fn resource(
    State((state, _)): State<WebState>,
    Path(resource): Path<String>,
) -> Result<Json<CodexResourceResponse>, HttpError> {
    let action = match resource.as_str() {
        "account" => ControlAction::AccountRead,
        "models" => ControlAction::ModelsList,
        "config" => ControlAction::ConfigRead,
        "mcp" => ControlAction::McpServersList,
        "plugins" => ControlAction::PluginsList,
        "skills" => ControlAction::SkillsList,
        "hooks" => ControlAction::HooksList,
        "apps" => ControlAction::AppsList,
        _ => {
            return Err(HttpError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "unknown Codex resource",
            ));
        }
    };
    let value = service(&state)?
        .read(action, serde_json::json!({}))
        .await
        .map_err(upstream)?;
    Ok(Json(CodexResourceResponse { resource, value }))
}

#[derive(Deserialize, ToSchema)]
pub struct CodexReadBody {
    action: ControlAction,
    #[serde(default)]
    params: Value,
}

pub async fn read_action(
    State((state, _)): State<WebState>,
    Json(body): Json<CodexReadBody>,
) -> Result<Json<CodexResourceResponse>, HttpError> {
    let method = body.action.method().to_string();
    let value = service(&state)?
        .read(body.action, body.params)
        .await
        .map_err(bad_request)?;
    Ok(Json(CodexResourceResponse {
        resource: method,
        value,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct CodexResourceResponse {
    resource: String,
    value: Value,
}

pub async fn create_operation(
    State((state, _)): State<WebState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<CreateOperationBody>,
) -> Result<Json<axon_services::codex_control::ControlOperation>, HttpError> {
    let intent = OperationIntent {
        actor: auth.sub.clone(),
        scope: auth.scopes.join(" "),
        method: body.action.method().to_string(),
        target_home_identity: String::new(),
        runtime_boot_id: 0,
        policy_version: String::new(),
        expected_revision: None,
        idempotency_key: body.idempotency_key,
        redacted_request: body.redacted_request,
    };
    service(&state)?
        .create_operation(body.action, &intent)
        .await
        .map(Json)
        .map_err(bad_request)
}

pub async fn list_operations(
    State((state, _)): State<WebState>,
) -> Result<Json<Vec<axon_services::codex_control::ControlOperation>>, HttpError> {
    service(&state)?
        .unfinished_operations()
        .map(Json)
        .map_err(upstream)
}

#[derive(Deserialize, ToSchema)]
pub struct CreateOperationBody {
    action: MutationAction,
    idempotency_key: String,
    redacted_request: Value,
}

pub async fn approve_operation(
    State((state, _)): State<WebState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<ApproveOperationResponse>, HttpError> {
    let capability = service(&state)?
        .approve_operation(id, &auth.sub)
        .map_err(bad_request)?;
    Ok(Json(ApproveOperationResponse {
        operation_id: id,
        approval_capability: capability,
    }))
}

pub async fn cancel_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
) -> Result<Json<ReconcileOperationResponse>, HttpError> {
    service(&state)?.cancel_operation(id).map_err(bad_request)?;
    Ok(Json(ReconcileOperationResponse {
        operation_id: id,
        phase: OperationPhase::Denied,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct ApproveOperationResponse {
    operation_id: i64,
    approval_capability: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ExecuteBody {
    capability: String,
    action: MutationAction,
    params: Value,
}
pub async fn execute_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
    Json(body): Json<ExecuteBody>,
) -> Result<Json<ExecuteOperationResponse>, HttpError> {
    let result = service(&state)?
        .execute_operation(id, &body.capability, body.action, body.params)
        .await
        .map_err(upstream)?;
    Ok(Json(ExecuteOperationResponse { result }))
}

#[derive(Serialize, ToSchema)]
pub struct ExecuteOperationResponse {
    result: Value,
}

#[derive(Deserialize, ToSchema)]
pub struct ServerRequestResponseBody {
    boot_id: u64,
    approved: bool,
    response: Option<Value>,
}

pub async fn respond_to_server_request(
    State((state, _)): State<WebState>,
    Path(id): Path<u64>,
    Json(body): Json<ServerRequestResponseBody>,
) -> Result<Json<ServerRequestRespondedResponse>, HttpError> {
    service(&state)?
        .respond_to_server_request(body.boot_id, id, body.approved, body.response)
        .await
        .map_err(upstream)?;
    Ok(Json(ServerRequestRespondedResponse {
        request_id: id,
        responded: true,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct ServerRequestRespondedResponse {
    request_id: u64,
    responded: bool,
}

pub async fn reconcile_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
    Json(body): Json<ReconcileOperationBody>,
) -> Result<Json<ReconcileOperationResponse>, HttpError> {
    if body.without_replay {
        service(&state)?
            .resolve_recovery_without_replay(
                id,
                body.effect_applied.ok_or_else(|| {
                    bad_request("effect_applied is required for non-replay disposition".into())
                })?,
                body.disposition_note.as_deref().unwrap_or_default(),
            )
            .map_err(bad_request)?;
    } else {
        service(&state)?
            .resolve_recovery(id)
            .await
            .map_err(bad_request)?;
    }
    Ok(Json(ReconcileOperationResponse {
        operation_id: id,
        phase: OperationPhase::Reconciled,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ReconcileOperationBody {
    #[serde(default)]
    without_replay: bool,
    effect_applied: Option<bool>,
    disposition_note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ReconcileOperationResponse {
    operation_id: i64,
    phase: OperationPhase,
}

fn upstream(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_GATEWAY, "bad_gateway", error)
}
fn bad_request(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_REQUEST, "bad_request", error)
}

#[utoipa::path(get, path = "/v1/codex", responses((status = 200, description = "Codex control runtime snapshot", body = axon_services::codex_control::CodexControlSnapshot), (status = 404, description = "Codex control disabled", body = super::super::error::ErrorBody), (status = 503, description = "Codex control unavailable", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn snapshot_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/events", params(("boot_id" = Option<u64>, Query, description = "Runtime boot identifier"), ("after" = Option<u64>, Query, description = "Last observed sequence"), ("limit" = Option<usize>, Query, description = "Maximum events")), responses((status = 200, description = "Bounded, redacted Codex event page", body = Vec<RecordedEvent>), (status = 502, description = "Codex app-server request failed", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn events_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/{resource}", params(("resource" = String, Path, description = "account, models, config, mcp, plugins, skills, hooks, or apps")), responses((status = 200, description = "Redacted Codex resource projection", body = CodexResourceResponse), (status = 404, description = "Unknown resource", body = super::super::error::ErrorBody), (status = 502, description = "Codex app-server request failed", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn resource_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/read", request_body = CodexReadBody, responses((status = 200, description = "Typed redacted Codex read result", body = CodexResourceResponse), (status = 400, description = "Unsupported read action or invalid parameters", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn read_action_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/operations", responses((status = 200, description = "Unfinished Codex control operations", body = Vec<axon_services::codex_control::ControlOperation>), (status = 502, description = "Codex operation store unavailable", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn list_operations_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations", request_body = CreateOperationBody, responses((status = 200, description = "Prepared Codex control operation", body = axon_services::codex_control::ControlOperation), (status = 400, description = "Invalid operation intent", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn create_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/approve", params(("id" = i64, Path, description = "Operation identifier")), responses((status = 200, description = "Short-lived approval capability", body = ApproveOperationResponse), (status = 400, description = "Operation cannot be approved", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn approve_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/cancel", params(("id" = i64, Path, description = "Operation identifier")), responses((status = 200, description = "Pending or approved operation cancelled", body = ReconcileOperationResponse), (status = 400, description = "Operation cannot be cancelled", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn cancel_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/execute", params(("id" = i64, Path, description = "Operation identifier")), request_body = ExecuteBody, responses((status = 200, description = "Sanitized Codex mutation result", body = ExecuteOperationResponse), (status = 502, description = "Codex app-server request failed", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn execute_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/server-requests/{id}/respond", params(("id" = u64, Path, description = "Pending server-request identifier")), request_body = ServerRequestResponseBody, responses((status = 200, description = "Single-use response accepted", body = ServerRequestRespondedResponse), (status = 502, description = "Unknown, expired, or incompatible request", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn respond_to_server_request_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/reconcile", params(("id" = i64, Path, description = "Operation identifier")), request_body = ReconcileOperationBody, responses((status = 200, description = "Ambiguous operation reconciled", body = ReconcileOperationResponse), (status = 400, description = "Operation cannot be reconciled", body = super::super::error::ErrorBody)), tag = "codex-control")]
#[allow(dead_code)]
pub async fn reconcile_operation_openapi_marker() {}

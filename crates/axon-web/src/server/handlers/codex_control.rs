use axon_codex::api::ControlAction;
use axon_codex::events::EventCursor;
use axon_codex::operations::OperationIntent;
use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lab_auth::AuthContext;
use serde::Deserialize;
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
) -> Result<Json<Vec<axon_codex::events::RecordedEvent>>, HttpError> {
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
) -> Result<Json<Value>, HttpError> {
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
    service(&state)?
        .read(action, serde_json::json!({}))
        .await
        .map(Json)
        .map_err(upstream)
}

pub async fn create_operation(
    State((state, _)): State<WebState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<CreateOperationBody>,
) -> Result<Json<axon_codex::operations::ControlOperation>, HttpError> {
    let intent = OperationIntent {
        actor: auth.sub.clone(),
        scope: auth.scopes.join(" "),
        method: body.method,
        target_home_identity: String::new(),
        runtime_boot_id: 0,
        policy_version: String::new(),
        expected_revision: body.expected_revision,
        idempotency_key: body.idempotency_key,
        redacted_request: body.redacted_request,
    };
    service(&state)?
        .create_operation(&intent)
        .await
        .map(Json)
        .map_err(bad_request)
}

pub async fn list_operations(
    State((state, _)): State<WebState>,
) -> Result<Json<Vec<axon_codex::operations::ControlOperation>>, HttpError> {
    service(&state)?
        .unfinished_operations()
        .map(Json)
        .map_err(upstream)
}

#[derive(Deserialize, ToSchema)]
pub struct CreateOperationBody {
    method: String,
    expected_revision: Option<String>,
    idempotency_key: String,
    redacted_request: Value,
}

pub async fn approve_operation(
    State((state, _)): State<WebState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, HttpError> {
    let capability = service(&state)?
        .approve_operation(id, &auth.sub)
        .map_err(bad_request)?;
    Ok(Json(
        serde_json::json!({"operation_id":id,"approval_capability":capability}),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ExecuteBody {
    capability: String,
    #[schema(value_type = String)]
    action: ControlAction,
    params: Value,
    revision: Option<String>,
}
pub async fn execute_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
    Json(body): Json<ExecuteBody>,
) -> Result<Json<Value>, HttpError> {
    service(&state)?
        .execute_operation(
            id,
            &body.capability,
            body.action,
            body.params,
            body.revision.as_deref(),
        )
        .await
        .map(Json)
        .map_err(upstream)
}

#[derive(Deserialize, ToSchema)]
pub struct ServerRequestResponseBody {
    boot_id: u64,
    approved: bool,
}

pub async fn respond_to_server_request(
    State((state, _)): State<WebState>,
    Path(id): Path<u64>,
    Json(body): Json<ServerRequestResponseBody>,
) -> Result<Json<Value>, HttpError> {
    service(&state)?
        .respond_to_server_request(body.boot_id, id, body.approved)
        .await
        .map_err(upstream)?;
    Ok(Json(serde_json::json!({"request_id":id,"responded":true})))
}

#[derive(Deserialize, ToSchema)]
pub struct ReconcileBody {
    revision: String,
}

pub async fn reconcile_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
    Json(body): Json<ReconcileBody>,
) -> Result<Json<Value>, HttpError> {
    service(&state)?
        .resolve_recovery(id, &body.revision)
        .map_err(bad_request)?;
    Ok(Json(
        serde_json::json!({"operation_id":id,"phase":"reconciled"}),
    ))
}

fn upstream(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_GATEWAY, "bad_gateway", error)
}
fn bad_request(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_REQUEST, "bad_request", error)
}

#[utoipa::path(get, path = "/v1/codex", responses((status = 200, description = "Codex control runtime snapshot"), (status = 404, description = "Codex control disabled"), (status = 503, description = "Codex control unavailable")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn snapshot_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/events", params(("boot_id" = Option<u64>, Query, description = "Runtime boot identifier"), ("after" = Option<u64>, Query, description = "Last observed sequence"), ("limit" = Option<usize>, Query, description = "Maximum events")), responses((status = 200, description = "Bounded, redacted Codex event page")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn events_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/{resource}", params(("resource" = String, Path, description = "account, models, config, mcp, plugins, skills, hooks, or apps")), responses((status = 200, description = "Redacted Codex resource projection"), (status = 404, description = "Unknown resource")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn resource_openapi_marker() {}

#[utoipa::path(get, path = "/v1/codex/operations", responses((status = 200, description = "Unfinished Codex control operations")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn list_operations_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations", request_body = CreateOperationBody, responses((status = 200, description = "Prepared Codex control operation"), (status = 400, description = "Invalid operation intent")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn create_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/approve", params(("id" = i64, Path, description = "Operation identifier")), responses((status = 200, description = "Short-lived approval capability"), (status = 400, description = "Operation cannot be approved")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn approve_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/execute", params(("id" = i64, Path, description = "Operation identifier")), request_body = ExecuteBody, responses((status = 200, description = "Sanitized Codex mutation result"), (status = 502, description = "Codex app-server request failed")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn execute_operation_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/server-requests/{id}/respond", params(("id" = u64, Path, description = "Pending server-request identifier")), request_body = ServerRequestResponseBody, responses((status = 200, description = "Single-use response accepted"), (status = 502, description = "Unknown, expired, or incompatible request")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn respond_to_server_request_openapi_marker() {}

#[utoipa::path(post, path = "/v1/codex/operations/{id}/reconcile", params(("id" = i64, Path, description = "Operation identifier")), request_body = ReconcileBody, responses((status = 200, description = "Ambiguous operation reconciled"), (status = 400, description = "Operation cannot be reconciled")), tag = "codex-control")]
#[allow(dead_code)]
pub async fn reconcile_operation_openapi_marker() {}

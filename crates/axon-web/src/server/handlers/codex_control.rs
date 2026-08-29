use axon_codex::api::ControlAction;
use axon_codex::events::EventCursor;
use axon_codex::operations::OperationIntent;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

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
    Json(body): Json<CreateOperationBody>,
) -> Result<Json<axon_codex::operations::ControlOperation>, HttpError> {
    let intent = OperationIntent {
        actor: body.actor,
        scope: body.scope,
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

#[derive(Deserialize)]
pub struct CreateOperationBody {
    actor: String,
    scope: String,
    method: String,
    expected_revision: Option<String>,
    idempotency_key: String,
    redacted_request: Value,
}

#[derive(Deserialize)]
pub struct ApproveBody {
    approver: String,
}
pub async fn approve_operation(
    State((state, _)): State<WebState>,
    Path(id): Path<i64>,
    Json(body): Json<ApproveBody>,
) -> Result<Json<Value>, HttpError> {
    let capability = service(&state)?
        .approve_operation(id, &body.approver)
        .map_err(bad_request)?;
    Ok(Json(
        serde_json::json!({"operation_id":id,"approval_capability":capability}),
    ))
}

#[derive(Deserialize)]
pub struct ExecuteBody {
    capability: String,
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

fn upstream(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_GATEWAY, "bad_gateway", error)
}
fn bad_request(error: String) -> HttpError {
    HttpError::new(StatusCode::BAD_REQUEST, "bad_request", error)
}

use crate::core::config::Config;
use crate::mcp::auth::authorize_mcp_http_headers_from_env;
use crate::services::action_api::dispatch_action;
use crate::services::context::ServiceContext;
use crate::services::types::{
    ClientActionError, ClientActionRequest, ClientActionResponse, ServerInfo,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::OnceCell;

const ACTIONS_BODY_LIMIT: usize = 128 * 1024;

#[derive(Clone)]
pub(crate) struct ActionState {
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
}

impl ActionState {
    pub(crate) fn new(
        cfg: Arc<Config>,
        service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    ) -> Self {
        Self {
            cfg,
            service_context,
        }
    }

    async fn service_context(&self) -> Result<Arc<ServiceContext>, ClientActionError> {
        self.service_context
            .get_or_try_init(|| async {
                ServiceContext::new_with_workers(Arc::clone(&self.cfg))
                    .await
                    .map(Arc::new)
            })
            .await
            .map(Arc::clone)
            .map_err(|err| {
                ClientActionError::new(
                    "internal",
                    format!("failed to initialize service context: {err}"),
                    true,
                    None,
                )
            })
    }
}

pub(crate) fn router(
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
) -> Router {
    Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route(
            "/v1/actions",
            post(v1_actions).layer(DefaultBodyLimit::max(ACTIONS_BODY_LIMIT)),
        )
        .with_state(ActionState::new(cfg, service_context))
}

async fn v1_capabilities() -> Json<ServerInfo> {
    Json(ServerInfo::current())
}

async fn v1_actions(
    State(state): State<ActionState>,
    headers: HeaderMap,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let request_id = payload
        .as_ref()
        .ok()
        .and_then(|Json(value)| request_id_from_value(value));

    if authorize_mcp_http_headers_from_env(&headers).is_err() {
        return json_error(
            StatusCode::UNAUTHORIZED,
            request_id,
            ClientActionError::new("unauthorized", "unauthorized", false, None),
        );
    }

    let Json(value) = match payload {
        Ok(payload) => payload,
        Err(err) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                request_id,
                ClientActionError::new("invalid_request", err.to_string(), false, None),
            );
        }
    };

    let request_id = request_id_from_value(&value);
    let request: ClientActionRequest = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                request_id,
                ClientActionError::new(
                    "invalid_request",
                    format!("invalid action request: {err}"),
                    false,
                    Some("body must be { request_id, action } and action must match the Axon MCP schema".into()),
                ),
            );
        }
    };

    let service_context = match state.service_context().await {
        Ok(ctx) => ctx,
        Err(err) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                Some(request.request_id),
                err,
            );
        }
    };

    match dispatch_action(&service_context, request.action).await {
        Ok(result) => Json(ClientActionResponse::ok(request.request_id, result)).into_response(),
        Err(err) => {
            let status = status_for_error(&err);
            json_error(status, Some(request.request_id), err)
        }
    }
}

fn request_id_from_value(value: &Value) -> Option<String> {
    value
        .get("request_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn status_for_error(error: &ClientActionError) -> StatusCode {
    match error.kind.as_str() {
        "invalid_request" | "unsupported_action" => StatusCode::BAD_REQUEST,
        "unauthorized" => StatusCode::UNAUTHORIZED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn json_error(
    status: StatusCode,
    request_id: Option<String>,
    error: ClientActionError,
) -> Response {
    (status, Json(ClientActionResponse::error(request_id, error))).into_response()
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;

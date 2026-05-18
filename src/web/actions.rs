use crate::mcp::auth::AuthPolicy;
use crate::services::action_api::{dispatch_action, required_scope};
use crate::services::context::ServiceContext;
use crate::services::types::{
    ClientActionError, ClientActionRequest, ClientActionResponse, ServerInfo,
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use lab_auth::AuthContext;
use serde_json::Value;
use std::sync::Arc;

const ACTIONS_BODY_LIMIT: usize = 128 * 1024;

#[derive(Clone)]
pub(crate) struct ActionState {
    service_context: Arc<ServiceContext>,
    auth_required: bool,
}

impl ActionState {
    pub(crate) fn new(service_context: Arc<ServiceContext>, auth_policy: AuthPolicy) -> Self {
        Self {
            service_context,
            auth_required: !matches!(auth_policy, AuthPolicy::LoopbackDev),
        }
    }
}

pub(crate) fn router(service_context: Arc<ServiceContext>, auth_policy: AuthPolicy) -> Router {
    let state = ActionState::new(service_context, auth_policy.clone());
    let actions = Router::new()
        .route(
            "/v1/actions",
            post(v1_actions).layer(DefaultBodyLimit::max(ACTIONS_BODY_LIMIT)),
        )
        .with_state(state);

    Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .merge(actions)
}

async fn v1_capabilities() -> Json<ServerInfo> {
    Json(ServerInfo::current())
}

async fn v1_actions(
    State(state): State<ActionState>,
    auth: Option<Extension<AuthContext>>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let request_id = payload
        .as_ref()
        .ok()
        .and_then(|Json(value)| request_id_from_value(value));

    let Json(value) = match payload {
        Ok(payload) => payload,
        Err(err) => {
            return deprecated_response(json_error(
                StatusCode::BAD_REQUEST,
                request_id,
                ClientActionError::new("invalid_request", err.to_string(), false, None),
            ));
        }
    };

    let request_id = request_id_from_value(&value);
    let request: ClientActionRequest = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => {
            return deprecated_response(json_error(
                StatusCode::BAD_REQUEST,
                request_id,
                ClientActionError::new(
                    "invalid_request",
                    format!("invalid action request: {err}"),
                    false,
                    Some("body must be { request_id, action } and action must match the Axon MCP schema".into()),
                ),
            ));
        }
    };

    if let Err((status, err)) = authorize_action(
        &state,
        auth.as_ref().map(|Extension(ctx)| ctx),
        &request.action,
    ) {
        return deprecated_response(json_error(status, Some(request.request_id), err));
    }

    match dispatch_action(&state.service_context, request.action).await {
        Ok(result) => deprecated_response(
            Json(ClientActionResponse::ok(request.request_id, result)).into_response(),
        ),
        Err(err) => {
            let status = status_for_error(&err);
            deprecated_response(json_error(status, Some(request.request_id), err))
        }
    }
}

fn authorize_action(
    state: &ActionState,
    auth: Option<&AuthContext>,
    action: &crate::mcp::schema::AxonRequest,
) -> Result<(), (StatusCode, ClientActionError)> {
    let force_auth = matches!(
        action,
        crate::mcp::schema::AxonRequest::Dedupe(_) | crate::mcp::schema::AxonRequest::Migrate(_)
    );
    if !state.auth_required && !force_auth {
        return Ok(());
    }
    let Some(auth) = auth else {
        return Err((
            StatusCode::UNAUTHORIZED,
            ClientActionError::new("unauthorized", "unauthorized", false, None),
        ));
    };
    let required_scope = required_scope(action).unwrap_or("axon:write");
    let allowed = auth.scopes.iter().any(|scope| {
        scope == required_scope || (required_scope == "axon:read" && scope == "axon:write")
    });
    if allowed {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            ClientActionError::new(
                "forbidden",
                format!("requires scope: {required_scope}"),
                false,
                None,
            ),
        ))
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
        "forbidden" => StatusCode::FORBIDDEN,
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

fn deprecated_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        HeaderName::from_static("deprecation"),
        HeaderValue::from_static("true"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("sunset"),
        HeaderValue::from_static("Tue, 01 Sep 2026 00:00:00 GMT"),
    );
    response
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;

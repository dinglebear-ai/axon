use crate::core::config::Config;
use crate::mcp::auth::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use crate::services::action_api::{dispatch_action, required_scope};
use crate::services::context::ServiceContext;
use crate::services::types::{
    ClientActionError, ClientActionRequest, ClientActionResponse, ServerInfo,
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use lab_auth::AuthContext;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::OnceCell;

const ACTIONS_BODY_LIMIT: usize = 128 * 1024;

#[derive(Clone)]
pub(crate) struct ActionState {
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    auth_required: bool,
}

impl ActionState {
    pub(crate) fn new(
        cfg: Arc<Config>,
        service_context: Arc<OnceCell<Arc<ServiceContext>>>,
        auth_policy: AuthPolicy,
    ) -> Self {
        Self {
            cfg,
            service_context,
            auth_required: !matches!(auth_policy, AuthPolicy::LoopbackDev),
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
    auth_policy: AuthPolicy,
) -> Router {
    let state = ActionState::new(Arc::clone(&cfg), service_context, auth_policy.clone());
    let actions = Router::new()
        .route(
            "/v1/actions",
            post(v1_actions).layer(DefaultBodyLimit::max(ACTIONS_BODY_LIMIT)),
        )
        .with_state(state);
    let actions = if let Some(layer) = build_auth_layer(
        &auth_policy,
        configured_mcp_http_token().map(Arc::from),
        oauth_resource_url(&auth_policy),
    ) {
        actions
            .layer(layer)
            .layer(middleware::from_fn(normalize_api_key_header))
            .layer(middleware::from_fn(jsonize_auth_error))
    } else {
        actions
    };

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

    if let Err((status, err)) = authorize_action(
        &state,
        auth.as_ref().map(|Extension(ctx)| ctx),
        &request.action,
    ) {
        return json_error(status, Some(request.request_id), err);
    }

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

async fn jsonize_auth_error(request: Request<Body>, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        let kind = if status == StatusCode::UNAUTHORIZED {
            "unauthorized"
        } else {
            "forbidden"
        };
        return json_error(
            status,
            None,
            ClientActionError::new(kind, kind, false, None),
        );
    }
    response
}

fn authorize_action(
    state: &ActionState,
    auth: Option<&AuthContext>,
    action: &crate::mcp::schema::AxonRequest,
) -> Result<(), (StatusCode, ClientActionError)> {
    // Destructive / irreversible actions require a valid token unconditionally —
    // they must NOT be reachable via LoopbackDev (no-token) mode regardless of
    // the global auth_required flag.
    let requires_unconditional_auth = matches!(
        action,
        crate::mcp::schema::AxonRequest::Migrate(_) | crate::mcp::schema::AxonRequest::Dedupe(_)
    );

    if !state.auth_required && !requires_unconditional_auth {
        return Ok(());
    }
    let Some(auth) = auth else {
        return Err((
            StatusCode::UNAUTHORIZED,
            ClientActionError::new("unauthorized", "unauthorized", false, None),
        ));
    };
    let Some(required_scope) = required_scope(action) else {
        return Ok(());
    };
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

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;

use crate::authz::scope_satisfies;
use crate::mcp::auth::{AuthPolicy, configured_mcp_http_token};
use crate::services::action_api::{dispatch_action, required_scope};
use crate::services::context::ServiceContext;
use crate::services::types::{
    ClientActionError, ClientActionRequest, ClientActionResponse, ServerInfo,
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
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
    Router::new()
        .route(
            "/v1/actions",
            post(v1_actions).layer(DefaultBodyLimit::max(ACTIONS_BODY_LIMIT)),
        )
        .with_state(state)
}

pub(crate) fn capabilities_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/v1/capabilities", get(v1_capabilities))
}

pub(crate) async fn v1_capabilities() -> Json<ServerInfo> {
    Json(ServerInfo::current())
}

async fn v1_actions(
    State(state): State<ActionState>,
    auth: Option<Extension<AuthContext>>,
    headers: HeaderMap,
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
        &headers,
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
    headers: &HeaderMap,
    action: &crate::mcp::schema::AxonRequest,
) -> Result<(), (StatusCode, ClientActionError)> {
    // Destructive / irreversible actions require a valid token unconditionally —
    // must NOT be reachable via LoopbackDev (no-token) mode regardless of the
    // global auth_required flag. INVARIANT: required_scope() in action_api.rs
    // must return Some(...) for every action listed here.
    let force_auth = matches!(
        action,
        crate::mcp::schema::AxonRequest::Dedupe(_) | crate::mcp::schema::AxonRequest::Migrate(_)
    );
    if !state.auth_required && !force_auth {
        return Ok(());
    }
    if static_token_matches(headers) {
        return Ok(());
    }
    let Some(auth) = auth else {
        return Err((
            StatusCode::UNAUTHORIZED,
            ClientActionError::new("unauthorized", "unauthorized", false, None),
        ));
    };
    let required_scope = required_scope(action).unwrap_or("axon:write");
    let allowed = scope_satisfies(&auth.scopes, required_scope);
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

fn static_token_matches(headers: &HeaderMap) -> bool {
    let Some(expected) = configured_mcp_http_token() else {
        return false;
    };
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
        .is_some_and(|token| token == expected);
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| token == expected);
    bearer || api_key
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("Bearer") {
        Some(token.trim())
    } else {
        None
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

// RFC 8594 Deprecation + Sunset + Link headers on every /v1/actions response.
// Link advertises the successor REST surface so automated clients can migrate.
fn deprecated_response(mut response: Response) -> Response {
    let h = response.headers_mut();
    h.insert(
        HeaderName::from_static("deprecation"),
        HeaderValue::from_static("true"),
    );
    h.insert(
        HeaderName::from_static("sunset"),
        HeaderValue::from_static("Tue, 01 Sep 2026 00:00:00 GMT"),
    );
    h.insert(
        HeaderName::from_static("link"),
        HeaderValue::from_static(
            "</v1/capabilities>; rel=\"alternate\"; type=\"application/json\", \
</v1/sources>; rel=\"successor-version\", \
</v1/query>; rel=\"successor-version\", \
</v1/crawl>; rel=\"successor-version\", \
</v1/embed>; rel=\"successor-version\", \
</v1/ingest>; rel=\"successor-version\", \
</v1/extract>; rel=\"successor-version\", \
</v1/migrate>; rel=\"successor-version\"",
        ),
    );
    response
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;

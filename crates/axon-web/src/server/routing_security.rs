use super::super::error::HttpError;
use super::loopback_guard::block_loopback_destructive_request;
use axon_authz::http::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use axon_authz::scope_satisfies;
use axum::{
    Extension, Router,
    body::Body,
    http::{HeaderValue, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
};
use lab_auth::AuthContext;
use std::sync::Arc;

pub(crate) const PANEL_AUTH_ISSUER: &str = "axon-panel";

pub(super) fn panel_auth_context() -> AuthContext {
    AuthContext {
        sub: "axon-panel".to_string(),
        actor_key: None,
        scopes: vec!["axon:read".to_string(), "axon:write".to_string()],
        issuer: PANEL_AUTH_ISSUER.to_string(),
        via_session: true,
        csrf_token: None,
        email: None,
    }
}

pub(super) async fn authenticate_panel_request(
    axum::extract::State(panel): axum::extract::State<Arc<crate::server::state::PanelRuntimeState>>,
    mut request: Request<Body>,
    next: middleware::Next,
) -> Response {
    let authorized = request
        .headers()
        .get("x-axon-panel-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| panel.password.verify(token));
    if !authorized {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    }
    request.extensions_mut().insert(panel_auth_context());
    next.run(request).await
}

#[derive(Clone, Copy)]
pub(super) enum ScopeRequirement {
    Read,
    Write,
    Admin,
}

pub(super) fn protect_routes<S>(
    router: Router<S>,
    auth_policy: &AuthPolicy,
    scope: ScopeRequirement,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let Some(layer) = build_auth_layer(
        auth_policy,
        configured_mcp_http_token().map(Arc::from),
        oauth_resource_url(auth_policy),
    ) else {
        return match (auth_policy, scope) {
            (AuthPolicy::LoopbackDev, ScopeRequirement::Write | ScopeRequirement::Admin) => {
                router.route_layer(middleware::from_fn(block_loopback_destructive_request))
            }
            _ => router,
        };
    };
    let router = match scope {
        ScopeRequirement::Read => router.route_layer(middleware::from_fn(require_read_scope)),
        ScopeRequirement::Write => router.route_layer(middleware::from_fn(require_write_scope)),
        ScopeRequirement::Admin => router.route_layer(middleware::from_fn(require_admin_scope)),
    };
    router
        .route_layer(layer)
        .route_layer(middleware::from_fn(normalize_api_key_header))
        .route_layer(middleware::from_fn(jsonize_auth_error))
}

async fn jsonize_auth_error(request: Request<Body>, next: middleware::Next) -> Response {
    let mut response = next.run(request).await;
    let status = response.status();
    if status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN {
        return response;
    }
    if response
        .headers_mut()
        .remove(super::super::api_error::ERROR_ENVELOPE_MARKER)
        .is_some()
    {
        return response;
    }
    let kind = if status == StatusCode::UNAUTHORIZED {
        "unauthorized"
    } else {
        "forbidden"
    };
    HttpError::new(status, kind, kind).into_response()
}

async fn require_read_scope(
    auth: Option<Extension<AuthContext>>,
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    require_scope(auth, "axon:read", request, next).await
}

async fn require_write_scope(
    auth: Option<Extension<AuthContext>>,
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    require_scope(auth, "axon:write", request, next).await
}

async fn require_admin_scope(
    auth: Option<Extension<AuthContext>>,
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    require_scope(auth, "axon:admin", request, next).await
}

async fn require_scope(
    auth: Option<Extension<AuthContext>>,
    required_scope: &'static str,
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    let Some(Extension(auth)) = auth else {
        return HttpError::new(StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized")
            .into_response();
    };
    if !scope_satisfies(&auth.scopes, required_scope) {
        return HttpError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("requires scope: {required_scope}"),
        )
        .into_response();
    }
    next.run(request).await
}

pub(super) async fn security_headers(request: Request<Body>, next: middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .remove(super::super::api_error::ERROR_ENVELOPE_MARKER);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}

#[cfg(test)]
#[path = "routing_security_tests.rs"]
mod tests;

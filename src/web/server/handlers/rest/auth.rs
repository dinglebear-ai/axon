//! Per-route scope guard middleware for REST handlers.
//!
//! `build_auth_layer` already installs the bearer/JWT auth layer once at the
//! router root; this middleware runs after that and enforces a single
//! `axon:read` / `axon:write` scope per route.
//!
//! Two flavors:
//!   - [`scope_guard`] — honors `auth_required=false` (LoopbackDev) and lets
//!     unauthenticated requests through. Used for non-destructive surfaces.
//!   - [`unconditional_scope_guard`] — always requires a valid `AuthContext`
//!     regardless of policy. Used for destructive admin routes (migrate,
//!     dedupe) per the invariant documented at
//!     `src/web/actions.rs:authorize_action`.

use super::error::rest_error;
use crate::authz::scope_satisfies;
use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use lab_auth::AuthContext;

pub(crate) fn scope_for_rest_route(method: &str, path: &str) -> Option<&'static str> {
    let scope = match (method, path) {
        ("GET", p) if p.starts_with("/v1/") => crate::mcp::auth::AxonScope::Read,
        ("POST", "/v1/query" | "/v1/retrieve" | "/v1/map") => crate::mcp::auth::AxonScope::Read,
        ("POST", "/v1/migrate" | "/v1/dedupe") => crate::mcp::auth::AxonScope::Write,
        ("POST", p) if p.starts_with("/v1/") => crate::mcp::auth::AxonScope::Write,
        ("DELETE", p) if p.starts_with("/v1/") => crate::mcp::auth::AxonScope::Write,
        _ => return None,
    };
    Some(scope.as_str())
}

/// Marker header attached to every scope-guard-rejected response. The outer
/// [`jsonize_auth_error`] middleware uses it to distinguish our richer JSON
/// envelopes (which carry the required scope name) from generic auth-layer
/// 401/403s that need to be normalized.
const SCOPE_GUARD_HEADER: &str = "x-axon-scope-guard";

fn tag_scope_guard(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(SCOPE_GUARD_HEADER, HeaderValue::from_static("1"));
    response
}

#[derive(Clone, Copy)]
pub(crate) struct ScopeGuard {
    pub required_scope: &'static str,
    pub auth_required: bool,
    pub unconditional: bool,
}

impl ScopeGuard {
    #[allow(dead_code)] // Used by Family 2/4 routes (sync POST + admin)
    pub(crate) const fn read(auth_required: bool) -> Self {
        Self {
            required_scope: "axon:read",
            auth_required,
            unconditional: false,
        }
    }

    #[allow(dead_code)] // Used by Family 2/3 routes
    pub(crate) const fn write(auth_required: bool) -> Self {
        Self {
            required_scope: "axon:write",
            auth_required,
            unconditional: false,
        }
    }

    /// Admin route — destructive, must require a token even in LoopbackDev.
    #[allow(dead_code)] // Used by Family 4 admin routes
    pub(crate) const fn admin_write() -> Self {
        Self {
            required_scope: "axon:write",
            auth_required: true,
            unconditional: true,
        }
    }
}

pub(crate) async fn enforce_scope(guard: ScopeGuard, request: Request, next: Next) -> Response {
    if !guard.auth_required && !guard.unconditional {
        return next.run(request).await;
    }
    let Some(auth) = request.extensions().get::<AuthContext>().cloned() else {
        return tag_scope_guard(rest_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "unauthorized".into(),
        ));
    };
    let allowed = scope_satisfies(&auth.scopes, guard.required_scope);
    if !allowed {
        return tag_scope_guard(rest_error(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("requires scope: {}", guard.required_scope),
        ));
    }
    next.run(request).await
}

/// Map any 401/403 produced by the auth layer (lab-auth or scope-guard
/// fallthrough) to our JSON error envelope.
///
/// Skips responses tagged with [`SCOPE_GUARD_HEADER`] — those are richer
/// JSON bodies emitted by [`enforce_scope`] that already carry the required
/// scope name and would lose that information if this generic normalizer
/// overwrote them. lab-auth's responses do not carry the marker and are
/// rewritten to the canonical `{ kind, message }` shape.
pub(crate) async fn jsonize_auth_error(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN {
        return response;
    }
    if response.headers().contains_key(SCOPE_GUARD_HEADER) {
        // Strip the internal marker before forwarding to the client — it is
        // an implementation detail that should not be visible in API responses.
        let mut response = response;
        response.headers_mut().remove(SCOPE_GUARD_HEADER);
        return response;
    }
    let kind = if status == StatusCode::UNAUTHORIZED {
        "unauthorized"
    } else {
        "forbidden"
    };
    rest_error(status, kind, kind.into())
}

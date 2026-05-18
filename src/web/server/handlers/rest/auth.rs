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
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use lab_auth::AuthContext;

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
        return rest_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "unauthorized".into(),
        );
    };
    let allowed = auth.scopes.iter().any(|scope| {
        scope == guard.required_scope
            || (guard.required_scope == "axon:read" && scope == "axon:write")
    });
    if !allowed {
        return rest_error(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("requires scope: {}", guard.required_scope),
        );
    }
    next.run(request).await
}

/// Map any 401/403 produced by the auth layer to our JSON error envelope.
pub(crate) async fn jsonize_auth_error(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        return rest_error(status, "unauthorized", "unauthorized".into());
    }
    if status == StatusCode::FORBIDDEN {
        return rest_error(status, "forbidden", "forbidden".into());
    }
    response
}

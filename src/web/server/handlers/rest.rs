//! Dedicated per-resource REST routes (`/v1/{resource}`) introduced in v4.x.
//!
//! Replaces the generic `POST /v1/actions` envelope dispatcher with one route
//! per surface using standard HTTP semantics — GET for read-only, POST for
//! mutations, POST + GET for async jobs.
//!
//! `/v1/actions` is kept for back-compat (see deprecation header in
//! `crate::web::actions`); these routes are the path forward.

#[path = "rest/auth.rs"]
pub(crate) mod auth;
#[path = "rest/error.rs"]
pub(crate) mod error;
#[path = "rest/read_only.rs"]
pub(crate) mod read_only;
#[path = "rest/state.rs"]
pub(crate) mod state;

use crate::core::config::Config;
use crate::mcp::auth::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use crate::services::context::ServiceContext;
use axum::{Router, middleware, routing::get};
use std::sync::Arc;
use tokio::sync::OnceCell;

use self::auth::{ScopeGuard, enforce_scope, jsonize_auth_error};
use self::state::RestState;

/// Build the REST `/v1/*` sub-router and merge it with `actions::router`'s
/// auth layer so the same `/v1` namespace shares one auth boundary.
pub(crate) fn router(
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    auth_policy: AuthPolicy,
) -> Router {
    let state = RestState::new(Arc::clone(&cfg), service_context, &auth_policy);

    let read_guard = ScopeGuard::read(state.auth_required);

    let rest = Router::new()
        .route(
            "/v1/sources",
            get(read_only::v1_sources).layer(middleware::from_fn(move |req, next| {
                enforce_scope(read_guard, req, next)
            })),
        )
        .route(
            "/v1/domains",
            get(read_only::v1_domains).layer(middleware::from_fn(move |req, next| {
                enforce_scope(read_guard, req, next)
            })),
        )
        .route(
            "/v1/stats",
            get(read_only::v1_stats).layer(middleware::from_fn(move |req, next| {
                enforce_scope(read_guard, req, next)
            })),
        )
        .route(
            "/v1/doctor",
            get(read_only::v1_doctor).layer(middleware::from_fn(move |req, next| {
                enforce_scope(read_guard, req, next)
            })),
        )
        .route(
            "/v1/status",
            get(read_only::v1_status).layer(middleware::from_fn(move |req, next| {
                enforce_scope(read_guard, req, next)
            })),
        )
        .with_state(state);

    if let Some(layer) = build_auth_layer(
        &auth_policy,
        configured_mcp_http_token().map(Arc::from),
        oauth_resource_url(&auth_policy),
    ) {
        rest.layer(layer)
            .layer(middleware::from_fn(normalize_api_key_header))
            .layer(middleware::from_fn(jsonize_auth_error))
    } else {
        rest
    }
}

#[cfg(test)]
#[path = "rest_tests.rs"]
mod tests;

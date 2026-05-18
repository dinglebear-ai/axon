//! Dedicated per-resource REST routes (`/v1/{resource}`) introduced in v4.x.
//!
//! Replaces the generic `POST /v1/actions` envelope dispatcher with one route
//! per surface using standard HTTP semantics — GET for read-only, POST for
//! mutations, POST + GET for async jobs.
//!
//! `/v1/actions` is kept for back-compat (see deprecation header in
//! `crate::web::actions`); these routes are the path forward.

#[path = "rest/admin.rs"]
pub(crate) mod admin;
#[path = "rest/async_jobs.rs"]
pub(crate) mod async_jobs;
#[path = "rest/auth.rs"]
pub(crate) mod auth;
#[path = "rest/error.rs"]
pub(crate) mod error;
#[path = "rest/read_only.rs"]
pub(crate) mod read_only;
#[path = "rest/state.rs"]
pub(crate) mod state;
#[path = "rest/sync_post.rs"]
pub(crate) mod sync_post;
#[path = "rest/types.rs"]
pub(crate) mod types;

use crate::core::config::Config;
use crate::mcp::auth::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use crate::services::context::ServiceContext;
use axum::{
    Router, middleware,
    routing::{MethodRouter, get, post},
};
use std::sync::Arc;
use tokio::sync::OnceCell;

use self::auth::{ScopeGuard, enforce_scope, jsonize_auth_error};
use self::state::RestState;

/// Wrap a [`MethodRouter`] with a scope-guard middleware bound to a single
/// [`ScopeGuard`]. Used so route declarations stay one-line per route.
fn guarded(method: MethodRouter<RestState>, guard: ScopeGuard) -> MethodRouter<RestState> {
    method.layer(middleware::from_fn(move |req, next| {
        enforce_scope(guard, req, next)
    }))
}

/// Build the REST `/v1/*` sub-router. The same auth layer covers every route
/// in this family; per-route scope checks run after it.
pub(crate) fn router(
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    auth_policy: AuthPolicy,
) -> Router {
    let state = RestState::new(Arc::clone(&cfg), service_context, &auth_policy);

    let read = ScopeGuard::read(state.auth_required);
    let write = ScopeGuard::write(state.auth_required);

    let rest = Router::new()
        // Family 1 — read-only GET
        .route("/v1/sources", guarded(get(read_only::v1_sources), read))
        .route("/v1/domains", guarded(get(read_only::v1_domains), read))
        .route("/v1/stats", guarded(get(read_only::v1_stats), read))
        .route("/v1/doctor", guarded(get(read_only::v1_doctor), read))
        .route("/v1/status", guarded(get(read_only::v1_status), read))
        // Family 2 — sync POST (read scope: query/retrieve/map; write scope: rest)
        .route("/v1/query", guarded(post(sync_post::v1_query), read))
        .route("/v1/retrieve", guarded(post(sync_post::v1_retrieve), read))
        .route("/v1/map", guarded(post(sync_post::v1_map), read))
        // NOTE: /v1/evaluate intentionally NOT exposed here. `services::query::evaluate`
        // returns `Box<dyn Error>` (non-Send) and its internals hold non-Send values
        // across `.await` points (see vector/ops/commands/evaluate/streaming.rs).
        // Wiring a multi-thread axum handler against it requires Send-ifying the
        // entire evaluate error chain — tracked separately. Callers can still hit
        // the evaluate action via POST /v1/actions { action: { action: "evaluate", ... } }.
        .route("/v1/suggest", guarded(post(sync_post::v1_suggest), write))
        .route("/v1/search", guarded(post(sync_post::v1_search), write))
        .route("/v1/research", guarded(post(sync_post::v1_research), write))
        .route("/v1/scrape", guarded(post(sync_post::v1_scrape), write))
        // Family 3 — async jobs (POST submit + GET status; cancel via POST .../:id/cancel
        // — DELETE is not used so the GET (read) and cancel (write) routes can carry
        // distinct scope guards without sharing one MethodRouter layer.)
        .route(
            "/v1/crawl",
            guarded(post(async_jobs::v1_crawl_submit), write),
        )
        .route(
            "/v1/crawl/{id}",
            guarded(get(async_jobs::v1_crawl_status), read),
        )
        .route(
            "/v1/crawl/{id}/cancel",
            guarded(post(async_jobs::v1_crawl_cancel), write),
        )
        .route(
            "/v1/embed",
            guarded(post(async_jobs::v1_embed_submit), write),
        )
        .route(
            "/v1/embed/{id}",
            guarded(get(async_jobs::v1_embed_status), read),
        )
        .route(
            "/v1/embed/{id}/cancel",
            guarded(post(async_jobs::v1_embed_cancel), write),
        )
        .route(
            "/v1/extract",
            guarded(post(async_jobs::v1_extract_submit), write),
        )
        .route(
            "/v1/extract/{id}",
            guarded(get(async_jobs::v1_extract_status), read),
        )
        .route(
            "/v1/extract/{id}/cancel",
            guarded(post(async_jobs::v1_extract_cancel), write),
        )
        .route(
            "/v1/ingest",
            guarded(post(async_jobs::v1_ingest_submit), write),
        )
        .route(
            "/v1/ingest/{id}",
            guarded(get(async_jobs::v1_ingest_status), read),
        )
        .route(
            "/v1/ingest/{id}/cancel",
            guarded(post(async_jobs::v1_ingest_cancel), write),
        )
        // Family 4 — admin / destructive. migrate + dedupe unconditionally
        // require auth (admin_write guard) even in LoopbackDev. Watch CRUD uses
        // standard read/write guards.
        .route(
            "/v1/migrate",
            guarded(post(admin::v1_migrate), ScopeGuard::admin_write()),
        )
        .route(
            "/v1/dedupe",
            guarded(post(admin::v1_dedupe), ScopeGuard::admin_write()),
        )
        // /v1/watch GET (list) is read-only; v1_watch_create is exposed as
        // POST /v1/watch/create so the two scope guards don't share a layer.
        .route("/v1/watch", guarded(get(admin::v1_watch_list), read))
        .route(
            "/v1/watch/create",
            guarded(post(admin::v1_watch_create), write),
        )
        .route("/v1/watch/{id}", guarded(get(admin::v1_watch_get), read))
        .route(
            "/v1/watch/{id}/run",
            guarded(post(admin::v1_watch_run_now), write),
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

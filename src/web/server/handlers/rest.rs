//! REST route module — provides the scope-guard middleware infrastructure
//! (ScopeGuard, enforce_scope, jsonize_auth_error) and a test-facing router.
//!
//! The per-resource HTTP routes are wired in main's canonical handler files
//! (discovery.rs, exploration.rs, async_jobs.rs, admin.rs, rag.rs). This
//! module is retained for the test suite: rest_tests.rs calls rest::router()
//! directly to exercise scope-guard middleware without a full server.
#![allow(dead_code, unused_imports)]
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
    routing::{MethodRouter, delete, get, post},
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

// ── Per-family route builders ────────────────────────────────────────────

fn family_1_read_only(read: ScopeGuard) -> Router<RestState> {
    Router::new()
        .route("/v1/sources", guarded(get(read_only::v1_sources), read))
        .route("/v1/domains", guarded(get(read_only::v1_domains), read))
        .route("/v1/stats", guarded(get(read_only::v1_stats), read))
        .route("/v1/doctor", guarded(get(read_only::v1_doctor), read))
        .route("/v1/status", guarded(get(read_only::v1_status), read))
}

fn family_2_sync_post(read: ScopeGuard, write: ScopeGuard) -> Router<RestState> {
    Router::new()
        .route("/v1/query", guarded(post(sync_post::v1_query), read))
        .route("/v1/retrieve", guarded(post(sync_post::v1_retrieve), read))
        .route("/v1/map", guarded(post(sync_post::v1_map), read))
        // NOTE: /v1/evaluate intentionally NOT exposed here. `services::query::evaluate`
        // returns `Box<dyn Error>` (non-Send) and its internals hold non-Send values
        // across `.await` points (see vector/ops/commands/evaluate/streaming.rs).
        // Wiring a multi-thread axum handler against it requires Send-ifying the
        // entire evaluate error chain — tracked separately.
        .route("/v1/suggest", guarded(post(sync_post::v1_suggest), write))
        .route("/v1/search", guarded(post(sync_post::v1_search), write))
        .route("/v1/research", guarded(post(sync_post::v1_research), write))
        .route("/v1/scrape", guarded(post(sync_post::v1_scrape), write))
        .route(
            "/v1/summarize",
            guarded(post(sync_post::v1_summarize), write),
        )
}

/// Cancel is POST .../cancel rather than DELETE /{id} so the GET (read) and
/// cancel (write) routes can carry distinct scope guards — axum 0.8
/// `MethodRouter` layers apply across all methods on a single path.
fn family_3_async_jobs(read: ScopeGuard, write: ScopeGuard) -> Router<RestState> {
    Router::new()
        .route(
            "/v1/crawl",
            guarded(post(async_jobs::v1_crawl_submit), write)
                .merge(guarded(get(async_jobs::v1_crawl_list), read))
                .merge(guarded(delete(async_jobs::v1_crawl_clear), write)),
        )
        .route(
            "/v1/crawl/cleanup",
            guarded(post(async_jobs::v1_crawl_cleanup), write),
        )
        .route(
            "/v1/crawl/recover",
            guarded(post(async_jobs::v1_crawl_recover), write),
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
            guarded(post(async_jobs::v1_embed_submit), write)
                .merge(guarded(get(async_jobs::v1_embed_list), read))
                .merge(guarded(delete(async_jobs::v1_embed_clear), write)),
        )
        .route(
            "/v1/embed/cleanup",
            guarded(post(async_jobs::v1_embed_cleanup), write),
        )
        .route(
            "/v1/embed/recover",
            guarded(post(async_jobs::v1_embed_recover), write),
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
            guarded(post(async_jobs::v1_extract_submit), write)
                .merge(guarded(get(async_jobs::v1_extract_list), read))
                .merge(guarded(delete(async_jobs::v1_extract_clear), write)),
        )
        .route(
            "/v1/extract/cleanup",
            guarded(post(async_jobs::v1_extract_cleanup), write),
        )
        .route(
            "/v1/extract/recover",
            guarded(post(async_jobs::v1_extract_recover), write),
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
            guarded(post(async_jobs::v1_ingest_submit), write)
                .merge(guarded(get(async_jobs::v1_ingest_list), read))
                .merge(guarded(delete(async_jobs::v1_ingest_clear), write)),
        )
        .route(
            "/v1/ingest/cleanup",
            guarded(post(async_jobs::v1_ingest_cleanup), write),
        )
        .route(
            "/v1/ingest/recover",
            guarded(post(async_jobs::v1_ingest_recover), write),
        )
        .route(
            "/v1/ingest/{id}",
            guarded(get(async_jobs::v1_ingest_status), read),
        )
        .route(
            "/v1/ingest/{id}/cancel",
            guarded(post(async_jobs::v1_ingest_cancel), write),
        )
}

#[cfg(test)]
pub(crate) fn documented_rest_paths_for_tests() -> Vec<String> {
    [
        "GET /v1/crawl",
        "POST /v1/crawl",
        "POST /v1/crawl/cleanup",
        "DELETE /v1/crawl",
        "POST /v1/crawl/recover",
        "GET /v1/embed",
        "POST /v1/embed",
        "POST /v1/embed/cleanup",
        "DELETE /v1/embed",
        "POST /v1/embed/recover",
        "GET /v1/extract",
        "POST /v1/extract",
        "POST /v1/extract/cleanup",
        "DELETE /v1/extract",
        "POST /v1/extract/recover",
        "GET /v1/ingest",
        "POST /v1/ingest",
        "POST /v1/ingest/cleanup",
        "DELETE /v1/ingest",
        "POST /v1/ingest/recover",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

/// Dedupe carries `admin_write` (unconditional auth even in LoopbackDev).
/// Watch list is read; watch create lives at /create so list and create can
/// carry distinct scope guards. Migrate remains CLI-only until it has a
/// dedicated async job family.
fn family_4_admin(read: ScopeGuard, write: ScopeGuard) -> Router<RestState> {
    Router::new()
        .route(
            "/v1/dedupe",
            guarded(post(admin::v1_dedupe), ScopeGuard::admin_write()),
        )
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
}

// ── Public router ────────────────────────────────────────────────────────

/// Build the REST `/v1/*` sub-router. The same auth layer covers every route;
/// per-route scope checks run after it.
pub(crate) fn router(
    cfg: Arc<Config>,
    service_context: Arc<OnceCell<Arc<ServiceContext>>>,
    auth_policy: AuthPolicy,
) -> Router {
    let state = RestState::new(Arc::clone(&cfg), service_context, &auth_policy);
    let read = ScopeGuard::read(state.auth_required);
    let write = ScopeGuard::write(state.auth_required);

    let rest = Router::new()
        .merge(family_1_read_only(read))
        .merge(family_2_sync_post(read, write))
        .merge(family_3_async_jobs(read, write))
        .merge(family_4_admin(read, write))
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

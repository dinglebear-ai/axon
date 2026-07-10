//! Per-verb `/v1/memories` REST routes.
//!
//! Each handler extracts `AuthContext`/builds a `CallerContext` the same way
//! `handlers::sources::index_source` does (`caller_context_from_auth`), parses
//! its own request shape, and calls exactly one `axon-services::memory`
//! function — no duplicated lifecycle logic here. The broad `axon:read`/
//! `axon:write` gate is already enforced by the router's scope middleware
//! (`routing::protect_routes`); the per-handler `CallerContext` construction
//! is Task 2's defense-in-depth/observability parity requirement, matching
//! `sources.rs`'s pattern. `import_memories` is the one route with a real
//! per-request fine-grained scope check today: `mode: replace_scope` derives
//! `axon_services::memory::MemoryAuthz` from the caller's resolved scopes and
//! requires `axon:admin`, mirroring `admin::prune_exec`'s `PruneAuthz`
//! derivation.

use axon_api::source::{
    AuthMode, CallerContext, MemoryExportRequest, MemoryImportRequest, TransportKind, Visibility,
};
use axon_authz::VisibilityPolicy;
use axon_core::config::Config;
use axon_services as services;
use axon_services::client_contract::RestMemoryRequest;
use axon_services::types::ClientActionError;
use axum::{Extension, extract::Path, extract::State};
use lab_auth::AuthContext;
use std::sync::Arc;

use super::super::super::error::HttpError;
use super::super::super::json::Json;
use super::memory_error;

type WebState = (super::super::super::state::AppState, Arc<Config>);

/// Build the [`CallerContext`] for a memory request the same way
/// `handlers::sources::caller_context_from_auth` does. Observability-only for
/// most memory routes (no per-source `SafetyClass` to classify); the one
/// route with a real fine-grained decision (`import_memories`'s
/// `replace_scope` admin gate) derives its own `MemoryAuthz` directly from
/// `AuthContext` rather than through this `CallerContext`, so this helper
/// stays available for logging/parity and any future per-route check that
/// wants the fuller shape.
fn caller_context_from_auth(auth: &AuthContext) -> CallerContext {
    let auth_mode = if auth.sub == "static-bearer" {
        AuthMode::StaticToken
    } else {
        AuthMode::Oauth
    };
    let mut caller = CallerContext {
        caller_id: Some(auth.sub.clone()),
        transport: TransportKind::Rest,
        trusted_local: false,
        scopes: auth.scopes.clone(),
        visibility_ceiling: Visibility::Public,
        auth_mode,
        token_id: None,
        display_name: None,
    };
    caller.visibility_ceiling = VisibilityPolicy::new().ceiling_for(&caller);
    caller
}

/// Log the caller context so the per-handler auth extraction is observable
/// on every memory route, independent of whether that specific route also
/// gates on it (only `import_memories`'s `replace_scope` mode does today).
/// Mirrors `sources.rs`'s `tracing::warn!` on denial, minus the denial itself
/// (`import_memories` logs/returns its own 403 directly).
fn log_caller(route: &'static str, auth: Option<&Extension<AuthContext>>) {
    if let Some(Extension(auth)) = auth {
        let caller = caller_context_from_auth(auth);
        tracing::debug!(route, caller_id = ?caller.caller_id, scopes = ?caller.scopes, "memory route caller");
    }
}

async fn dispatch_subaction(
    state: &super::super::super::state::AppState,
    mut req: RestMemoryRequest,
    subaction: axon_services::client_contract::RestMemorySubaction,
) -> Result<serde_json::Value, ClientActionError> {
    req.subaction = Some(subaction);
    // `RestMemorySubaction` has no `Import` variant (import/export are
    // separate typed routes — see `import_memories`/`export_memories`
    // below), so `MemoryAuthz::anonymous()` is safe here: none of the
    // subactions reachable through this dispatcher consult it.
    services::memory::dispatch(
        &state.service_context,
        req.into(),
        &services::memory::MemoryAuthz::anonymous(),
    )
    .await
}

macro_rules! memory_route {
    ($name:ident, $method:ident, $path:literal, $subaction:ident) => {
        #[utoipa::path(
            $method,
            path = $path,
            request_body = RestMemoryRequest,
            responses(
                (status = 200, description = "Persistent memory result", body = serde_json::Value),
                (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
                (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
            ),
            tag = "memory"
        )]
        pub(crate) async fn $name(
            State((state, _cfg)): State<WebState>,
            auth: Option<Extension<AuthContext>>,
            Json(req): Json<RestMemoryRequest>,
        ) -> Result<Json<serde_json::Value>, HttpError> {
            log_caller($path, auth.as_ref());
            dispatch_subaction(
                &state,
                req,
                axon_services::client_contract::RestMemorySubaction::$subaction,
            )
            .await
            .map(Json)
            .map_err(memory_error)
        }
    };
}

memory_route!(remember_memory, post, "/v1/memories", Remember);
memory_route!(search_memories, post, "/v1/memories/search", Search);
memory_route!(memory_context, post, "/v1/memories/context", Context);
memory_route!(review_memories, post, "/v1/memories/review", Review);
memory_route!(compact_memories, post, "/v1/memories/compact", Compact);

#[utoipa::path(
    get,
    path = "/v1/memories/{memory_id}",
    params(("memory_id" = String, Path, description = "Memory id")),
    responses(
        (status = 200, description = "Persistent memory result", body = serde_json::Value),
        (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn show_memory(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Path(memory_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    log_caller("/v1/memories/{memory_id}", auth.as_ref());
    let req = RestMemoryRequest {
        id: Some(memory_id),
        ..empty_rest_request()
    };
    dispatch_subaction(
        &state,
        req,
        axon_services::client_contract::RestMemorySubaction::Show,
    )
    .await
    .map(Json)
    .map_err(memory_error)
}

macro_rules! memory_id_route {
    ($name:ident, $method:ident, $path:literal, $subaction:ident) => {
        #[utoipa::path(
            $method,
            path = $path,
            params(("memory_id" = String, Path, description = "Memory id")),
            request_body = RestMemoryRequest,
            responses(
                (status = 200, description = "Persistent memory result", body = serde_json::Value),
                (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
                (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
            ),
            tag = "memory"
        )]
        pub(crate) async fn $name(
            State((state, _cfg)): State<WebState>,
            auth: Option<Extension<AuthContext>>,
            Path(memory_id): Path<String>,
            Json(mut req): Json<RestMemoryRequest>,
        ) -> Result<Json<serde_json::Value>, HttpError> {
            log_caller($path, auth.as_ref());
            req.id = Some(memory_id);
            dispatch_subaction(
                &state,
                req,
                axon_services::client_contract::RestMemorySubaction::$subaction,
            )
            .await
            .map(Json)
            .map_err(memory_error)
        }
    };
}

memory_id_route!(
    supersede_memory,
    post,
    "/v1/memories/{memory_id}/supersede",
    Supersede
);
memory_id_route!(
    reinforce_memory,
    post,
    "/v1/memories/{memory_id}/reinforce",
    Reinforce
);
memory_id_route!(
    contradict_memory,
    post,
    "/v1/memories/{memory_id}/contradict",
    Contradict
);
memory_id_route!(pin_memory, post, "/v1/memories/{memory_id}/pin", Pin);
memory_id_route!(
    archive_memory,
    post,
    "/v1/memories/{memory_id}/archive",
    Archive
);
memory_id_route!(
    compact_one_memory,
    post,
    "/v1/memories/{memory_id}/compact",
    Compact
);

/// `POST /v1/memories/{memory_id}/link` — `source_id` is filled from the path
/// (the memory being linked), `target_id` from the body.
#[utoipa::path(
    post,
    path = "/v1/memories/{memory_id}/link",
    params(("memory_id" = String, Path, description = "Source memory id")),
    request_body = RestMemoryRequest,
    responses(
        (status = 200, description = "Persistent memory result", body = serde_json::Value),
        (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn link_memory(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Path(memory_id): Path<String>,
    Json(mut req): Json<RestMemoryRequest>,
) -> Result<Json<serde_json::Value>, HttpError> {
    log_caller("/v1/memories/{memory_id}/link", auth.as_ref());
    req.source_id = Some(memory_id);
    dispatch_subaction(
        &state,
        req,
        axon_services::client_contract::RestMemorySubaction::Link,
    )
    .await
    .map(Json)
    .map_err(memory_error)
}

/// `DELETE /v1/memories/{memory_id}` — forget. `reason` is accepted as an
/// optional JSON body for parity with the other id-scoped mutation routes.
#[utoipa::path(
    delete,
    path = "/v1/memories/{memory_id}",
    params(("memory_id" = String, Path, description = "Memory id")),
    responses(
        (status = 200, description = "Persistent memory result", body = serde_json::Value),
        (status = 400, description = "Invalid memory request", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn forget_memory(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Path(memory_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    log_caller("/v1/memories/{memory_id}", auth.as_ref());
    let req = RestMemoryRequest {
        id: Some(memory_id),
        ..empty_rest_request()
    };
    dispatch_subaction(
        &state,
        req,
        axon_services::client_contract::RestMemorySubaction::Forget,
    )
    .await
    .map(Json)
    .map_err(memory_error)
}

/// `POST /v1/memories/import` — bulk-import memory records with an explicit
/// body size limit (`large_write_routes` layers `DefaultBodyLimit::max` on
/// this route in `routing.rs`; axum's `Json` extractor maps the resulting
/// oversized-body rejection to `413 Payload Too Large`).
#[utoipa::path(
    post,
    path = "/v1/memories/import",
    request_body = MemoryImportRequest,
    responses(
        (status = 200, description = "Import result", body = axon_api::source::MemoryImportResult),
        (status = 400, description = "Invalid import request", body = crate::server::error::ErrorBody),
        (status = 403, description = "mode=replace_scope requires axon:admin", body = crate::server::error::ErrorBody),
        (status = 413, description = "Import payload exceeds the size limit", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn import_memories(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<MemoryImportRequest>,
) -> Result<Json<axon_api::source::MemoryImportResult>, HttpError> {
    log_caller("/v1/memories/import", auth.as_ref());
    // `mode: replace_scope` archives every existing memory in the target
    // scope before importing and is documented as requiring `axon:admin`
    // (`axon_api::source::MemoryImportMode::ReplaceScope`). The router's
    // `memory_bulk_routes` layer only enforces `axon:write` for this route,
    // so this is the actual enforcement point for the elevated mode —
    // derived from the caller's real resolved scopes, never hardcoded.
    // `LoopbackDev` has no `AuthContext` at all and is locally-trusted,
    // matching `prune_exec`'s and the CLI's own local-trust rationale.
    let authz = match auth.as_ref() {
        Some(Extension(auth_ctx)) => services::memory::MemoryAuthz {
            is_admin: axon_authz::scope_satisfies(&auth_ctx.scopes, axon_authz::AXON_ADMIN_SCOPE),
        },
        None => services::memory::MemoryAuthz::admin(),
    };
    if req.mode == axon_api::source::MemoryImportMode::ReplaceScope && !authz.is_admin {
        return Err(HttpError::new(
            axum::http::StatusCode::FORBIDDEN,
            "forbidden",
            "memory import mode 'replace_scope' requires axon:admin",
        ));
    }
    services::memory::import(&state.service_context, req, &authz)
        .await
        .map(Json)
        .map_err(anyhow_to_http_error)
}

/// `POST /v1/memories/export` — export memory records matching a scope, with
/// the same body-size layer as `import_memories` (the request body is small,
/// but the response can be large; the size limit bounds abusive requests
/// consistently across both routes).
#[utoipa::path(
    post,
    path = "/v1/memories/export",
    request_body = MemoryExportRequest,
    responses(
        (status = 200, description = "Export result", body = axon_api::source::MemoryExportResult),
        (status = 400, description = "Invalid export request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Export request payload exceeds the size limit", body = crate::server::error::ErrorBody),
        (status = 502, description = "Upstream vector or embedding service unavailable", body = crate::server::error::ErrorBody)
    ),
    tag = "memory"
)]
pub(crate) async fn export_memories(
    State((state, _cfg)): State<WebState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<MemoryExportRequest>,
) -> Result<Json<axon_api::source::MemoryExportResult>, HttpError> {
    log_caller("/v1/memories/export", auth.as_ref());
    services::memory::export(&state.service_context, req)
        .await
        .map(Json)
        .map_err(anyhow_to_http_error)
}

fn anyhow_to_http_error(err: impl std::fmt::Display) -> HttpError {
    HttpError::from_error(&std::io::Error::other(err.to_string()))
}

fn empty_rest_request() -> RestMemoryRequest {
    RestMemoryRequest {
        subaction: None,
        id: None,
        source_id: None,
        target_id: None,
        edge_type: None,
        memory_type: None,
        title: None,
        body: None,
        query: None,
        project: None,
        repo: None,
        file: None,
        status: None,
        confidence: None,
        limit: None,
        depth: None,
        token_budget: None,
        amount: None,
        pinned: None,
        reason: None,
        memory_ids: None,
        strategy: None,
        archive_sources: None,
    }
}

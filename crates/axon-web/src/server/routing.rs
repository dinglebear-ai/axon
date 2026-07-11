use super::error::HttpError;
use super::handlers;
use super::state::AppState;
use super::types::{ASK_BODY_LIMIT, MEMORY_IMPORT_EXPORT_BODY_LIMIT};
use axon_authz::http::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use axon_authz::scope_satisfies;
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::types::ServerInfo;
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use lab_auth::AuthContext;
use std::sync::Arc;

#[path = "routing_loopback_guard.rs"]
mod loopback_guard;
use loopback_guard::block_loopback_destructive_request;

#[path = "routing_resource_tier.rs"]
mod resource_tier;

/// The state type every `/v1` REST subrouter is built over.
type ServeState = (AppState, Arc<Config>);

pub(super) fn router(
    cfg: Arc<Config>,
    panel: Arc<crate::server::state::PanelRuntimeState>,
    service_context: Arc<ServiceContext>,
    auth_policy: AuthPolicy,
) -> Router {
    let state = AppState {
        panel,
        service_context: Arc::clone(&service_context),
    };
    let rest_routes = protect_routes(
        read_routes(Arc::clone(&cfg), Arc::clone(&service_context)),
        &auth_policy,
        ScopeRequirement::Read,
    )
    .merge(protect_routes(
        write_routes(Arc::clone(&cfg), &service_context),
        &auth_policy,
        ScopeRequirement::Write,
    ))
    .merge(protect_routes(
        large_write_routes(&service_context),
        &auth_policy,
        ScopeRequirement::Write,
    ))
    .merge(protect_routes(
        memory_bulk_routes(),
        &auth_policy,
        ScopeRequirement::Write,
    ))
    .merge(protect_routes(
        admin_routes(&service_context),
        &auth_policy,
        ScopeRequirement::Admin,
    ));
    Router::new()
        .route("/healthz", get(super::super::health::healthz))
        .route("/readyz", get(super::super::health::readyz))
        // Prometheus scrape endpoint — unauthenticated like the health probes,
        // so an in-cluster scraper can read it without a token.
        .route("/metrics", get(super::super::metrics::metrics_handler))
        // `/v1/actions` and `/v1/migrate` are intentionally NOT registered.
        // Per the REST contract's no-tombstone rule (U2-18), a removed/never-
        // exposed route is a plain 404 from `api_aware_not_found`, not a
        // dedicated remap-guidance handler.
        .merge(super::openapi::docs_router())
        .merge(panel_routes())
        .merge(rest_routes)
        // Unknown paths: API prefixes get the contract `ErrorEnvelope` 404;
        // everything else falls through to the SPA static-asset server.
        .fallback(api_aware_not_found)
        // Known path, wrong method → enveloped 405 (axum otherwise returns an
        // empty-body 405).
        .method_not_allowed_fallback(super::json::method_not_allowed_fallback)
        .layer(middleware::from_fn(security_headers))
        .with_state((state, Arc::clone(&cfg)))
}

/// Routes reachable with `axon:read` — metadata and pure retrieval, plus the
/// query-shaped surfaces from U2-20/C6-20 (ask/chat/search/research/
/// summarize/suggest/evaluate, and memory search/context) that default to
/// `axon:read` even though some may enqueue a background index/crawl job as
/// a side effect. There is no `required_scope_if`/`mutates_if` conditional-
/// upgrade metadata yet (tracked as a follow-up); until it lands these stay
/// permanently read-gated rather than write-gated, matching the contract's
/// stated default.
fn read_routes(cfg: Arc<Config>, service_context: Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/sources", get(handlers::discovery::sources))
        .merge(resource_tier::routes())
        .route("/v1/domains", get(handlers::discovery::domains))
        .route("/v1/stats", get(handlers::discovery::stats))
        .route("/v1/status", get(handlers::discovery::status))
        .route("/v1/doctor", get(handlers::discovery::doctor))
        .route("/v1/collections", get(handlers::collections))
        .route(
            "/v1/mobile/sessions",
            get(handlers::mobile_sessions::list_mobile_sessions),
        )
        .route(
            "/v1/mobile/sessions/{id}",
            get(handlers::mobile_sessions::get_mobile_session),
        )
        .route(
            "/v1/memories/{memory_id}",
            get(handlers::memory::show_memory),
        )
        .route("/v1/query", post(handlers::rag::query))
        .route("/v1/retrieve", post(handlers::rag::retrieve))
        .route("/v1/map", post(handlers::exploration::map))
        .route(
            "/v1/artifacts",
            get(handlers::artifacts::serve_artifact_query),
        )
        .route(
            "/v1/artifacts/{*path}",
            get(handlers::artifacts::serve_artifact_path),
        )
        .nest(
            "/v1/jobs",
            handlers::jobs::unified_jobs_read_router(Arc::clone(&service_context)),
        )
        .merge(ask_router::<ServeState>(
            Arc::clone(&cfg),
            Arc::clone(&service_context),
        ))
        .route("/v1/evaluate", post(handlers::rag::evaluate))
        .route("/v1/suggest", post(handlers::rag::suggest))
        .route("/v1/summarize", post(handlers::exploration::summarize))
        .route(
            "/v1/summarize/stream",
            post(handlers::exploration::summarize_stream),
        )
        .route("/v1/search", post(handlers::exploration::search))
        .route("/v1/research", post(handlers::exploration::research))
        .route(
            "/v1/research/stream",
            post(handlers::exploration::research_stream),
        )
        .route(
            "/v1/memories/search",
            post(handlers::memory::search_memories),
        )
        .route(
            "/v1/memories/context",
            post(handlers::memory::memory_context),
        )
        .route("/v1/watches", get(handlers::source_watch::list_watches))
        .route(
            "/v1/watches/{watch_id}",
            get(handlers::source_watch::get_watch),
        )
        .route("/v1/graph/kinds", get(handlers::graph::kinds))
        .route("/v1/graph/resolve", post(handlers::graph::resolve))
        .route("/v1/graph/query", post(handlers::graph::query))
        .route("/v1/graph/nodes/{node_id}", get(handlers::graph::get_node))
        .route(
            "/v1/graph/nodes/{node_id}/edges",
            get(handlers::graph::get_node_edges),
        )
        .route("/v1/graph/edges/{edge_id}", get(handlers::graph::get_edge))
        .route(
            "/v1/graph/sources/{source_id}",
            get(handlers::graph::get_source_subgraph),
        )
}

/// Routes requiring `axon:write` — active-network operations, job
/// submission, and destructive ops. Endpoint discovery fetches pages,
/// bundles, probes endpoints, and may execute Chrome capture — it must not
/// be accessible with read-only tokens.
fn write_routes(_cfg: Arc<Config>, service_context: &Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .route("/v1/endpoints", post(handlers::exploration::endpoints))
        .route("/v1/brand", post(handlers::exploration::brand))
        .route("/v1/diff", post(handlers::exploration::diff))
        .route("/v1/screenshot", post(handlers::exploration::screenshot))
        .route("/v1/sources", post(handlers::sources::index_source))
        .route("/v1/memory", post(handlers::memory::memory))
        .route("/v1/memories", post(handlers::memory::remember_memory))
        .route(
            "/v1/memories/review",
            post(handlers::memory::review_memories),
        )
        .route(
            "/v1/memories/compact",
            post(handlers::memory::compact_memories),
        )
        .route(
            "/v1/memories/{memory_id}/link",
            post(handlers::memory::link_memory),
        )
        .route(
            "/v1/memories/{memory_id}/supersede",
            post(handlers::memory::supersede_memory),
        )
        .route(
            "/v1/memories/{memory_id}/reinforce",
            post(handlers::memory::reinforce_memory),
        )
        .route(
            "/v1/memories/{memory_id}/contradict",
            post(handlers::memory::contradict_memory),
        )
        .route(
            "/v1/memories/{memory_id}/pin",
            post(handlers::memory::pin_memory),
        )
        .route(
            "/v1/memories/{memory_id}/archive",
            post(handlers::memory::archive_memory),
        )
        .route(
            "/v1/memories/{memory_id}/compact",
            post(handlers::memory::compact_one_memory),
        )
        .route(
            "/v1/memories/{memory_id}",
            delete(handlers::memory::forget_memory),
        )
        .nest(
            "/v1/jobs",
            handlers::jobs::unified_jobs_write_router(Arc::clone(service_context)),
        )
        .nest(
            "/v1/extract",
            handlers::async_jobs::extract_router(Arc::clone(service_context)),
        )
        .route(
            "/v1/watch",
            get(handlers::admin::list_watch).post(handlers::admin::create_watch),
        )
        .route("/v1/watch/{id}/run", post(handlers::admin::run_watch))
        .route("/v1/watches", post(handlers::source_watch::create_watch))
        .route(
            "/v1/watches/{watch_id}",
            axum::routing::patch(handlers::source_watch::update_watch)
                .delete(handlers::source_watch::delete_watch),
        )
        .route(
            "/v1/watches/{watch_id}/pause",
            post(handlers::source_watch::pause_watch),
        )
        .route(
            "/v1/watches/{watch_id}/resume",
            post(handlers::source_watch::resume_watch),
        )
        .layer(DefaultBodyLimit::max(128 * 1024))
}

/// Routes requiring the explicit `axon:admin` scope. Broad write tokens do not
/// satisfy this scope.
fn admin_routes(service_context: &Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .nest(
            "/v1/jobs",
            handlers::jobs::unified_jobs_admin_router(Arc::clone(service_context)),
        )
        .route("/v1/prune/plan", post(handlers::admin::prune_plan))
        .route("/v1/prune/exec", post(handlers::admin::prune_exec))
        .route("/v1/prune/dedupe", post(handlers::admin::dedupe))
        .route("/v1/prune/purge", post(handlers::admin::purge))
}

/// Write-scoped routes whose payloads exceed the standard REST body cap
/// (prepared session exports ship megabytes of transcript JSON).
fn large_write_routes(_service_context: &Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .route(
            "/v1/mobile/sessions/{id}",
            put(handlers::mobile_sessions::upsert_mobile_session)
                .delete(handlers::mobile_sessions::delete_mobile_session),
        )
        .layer(DefaultBodyLimit::max(96 * 1024 * 1024))
}

/// Bulk memory transfer routes with their own explicit body size limit,
/// distinct from `write_routes`'s 128 KiB cap (too small for a real import
/// bundle) and `large_write_routes`'s 96 MiB cap (too generous for memory
/// records — a prior security review flagged the originating draft for
/// shipping import/export with no size control at all).
fn memory_bulk_routes() -> Router<ServeState> {
    Router::new()
        .route(
            "/v1/memories/import",
            post(handlers::memory::import_memories),
        )
        .route(
            "/v1/memories/export",
            post(handlers::memory::export_memories),
        )
        .layer(DefaultBodyLimit::max(MEMORY_IMPORT_EXPORT_BODY_LIMIT))
}

/// Panel-scoped routes — all protected by the panel password session cookie.
fn panel_routes() -> Router<ServeState> {
    Router::new()
        .route("/api/panel/state", get(handlers::panel_state))
        .route("/api/panel/login", post(handlers::login))
        .route(
            "/api/panel/config",
            get(handlers::get_config).put(handlers::save_config),
        )
        .route(
            "/api/panel/env",
            get(handlers::get_env_config).put(handlers::save_env_config),
        )
        .route("/api/panel/status", get(handlers::panel_status))
        .route("/api/panel/doctor", get(handlers::panel_doctor))
        .route("/api/panel/command", post(handlers::panel_command))
        .route("/api/panel/ops", get(handlers::ops))
        .route("/api/panel/collections", get(handlers::panel_collections))
        .route(
            "/api/panel/stack",
            get(super::super::panel_stack::stack_status),
        )
        .route(
            "/api/panel/first-run/crawl",
            post(super::super::panel_first_run::first_run_crawl),
        )
        .route(
            "/api/panel/first-run/ask",
            post(super::super::panel_first_run::first_run_ask),
        )
        .route("/api/panel/setup/targets", get(handlers::setup_targets))
        .route("/api/panel/artifact/{*path}", get(handlers::panel_artifact))
}

#[utoipa::path(
    get,
    path = "/v1/capabilities",
    responses(
        (status = 200, description = "Server capability metadata", body = ServerInfo),
        (status = 401, description = "Missing or invalid auth", body = crate::server::error::ErrorBody),
        (status = 403, description = "Insufficient scope", body = crate::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(super) async fn v1_capabilities() -> Json<ServerInfo> {
    Json(ServerInfo::rest_capabilities())
}

/// Router fallback for unrouted paths.
///
/// API surfaces (`/v1/*`, `/api/*`) return the contract `ErrorEnvelope` 404 so
/// clients never receive the SPA `index.html` for a mistyped API route. All
/// other paths fall through to the static-asset SPA server (which itself serves
/// `index.html` for client-side routing).
async fn api_aware_not_found(uri: axum::http::Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/v1/") || path.starts_with("/api/") {
        return super::json::not_found_fallback().await;
    }
    super::super::static_assets::serve_static(uri).await
}

pub(crate) fn ask_router<S>(cfg: Arc<Config>, service_context: Arc<ServiceContext>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::<S>::new()
        .route("/v1/ask", post(handlers::v1_ask))
        .route("/v1/ask/stream", post(handlers::v1_ask_stream))
        .route("/v1/chat", post(handlers::v1_chat))
        .route("/v1/chat/stream", post(handlers::v1_chat_stream))
        .layer(DefaultBodyLimit::max(ASK_BODY_LIMIT))
        // `ask`/`ask/stream` read the runtime through this Extension (issue #298
        // retrieval cutover); `chat` handlers ignore it and use `cfg` only.
        .layer(Extension(service_context))
        .layer(Extension(cfg))
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
            (AuthPolicy::LoopbackDev, ScopeRequirement::Write) => {
                router.route_layer(middleware::from_fn(block_loopback_destructive_request))
            }
            (AuthPolicy::LoopbackDev, ScopeRequirement::Admin) => {
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
    // A response our own error boundary already built (handler `HttpError`,
    // per-source `auth.forbidden`, scope-guard) carries the envelope marker and
    // must NOT be flattened into a generic `{unauthorized|forbidden}` — that
    // would drop richer detail like `required_scope`. Only bare auth-layer
    // 401/403s (which lack the marker) get normalized here.
    if response
        .headers_mut()
        .remove(super::api_error::ERROR_ENVELOPE_MARKER)
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
    let allowed = scope_satisfies(&auth.scopes, required_scope);
    if !allowed {
        return HttpError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            format!("requires scope: {required_scope}"),
        )
        .into_response();
    }
    next.run(request).await
}

async fn security_headers(request: Request<Body>, next: middleware::Next) -> Response {
    let mut response = next.run(request).await;
    // Strip the internal error-envelope marker so it never reaches clients.
    // `jsonize_auth_error` already removes it on the auth paths it touches; this
    // is the catch-all for every other enveloped error response.
    response
        .headers_mut()
        .remove(super::api_error::ERROR_ENVELOPE_MARKER);
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

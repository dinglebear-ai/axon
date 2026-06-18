use super::error::HttpError;
use super::handlers;
use super::state::AppState;
use super::types::ASK_BODY_LIMIT;
use crate::authz::scope_satisfies;
use crate::core::config::Config;
use crate::mcp::auth::{
    AuthPolicy, build_auth_layer, configured_mcp_http_token, normalize_api_key_header,
    oauth_resource_url,
};
use crate::services::context::ServiceContext;
use crate::services::types::ServerInfo;
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use lab_auth::AuthContext;
use std::sync::Arc;

/// The state type every `/v1` REST subrouter is built over.
type ServeState = (AppState, Arc<Config>);

pub(super) fn router(
    cfg: Arc<Config>,
    panel: Arc<crate::web::server::state::PanelRuntimeState>,
    service_context: Arc<ServiceContext>,
    auth_policy: AuthPolicy,
) -> Router {
    let state = AppState {
        panel,
        service_context: Arc::clone(&service_context),
    };
    let rest_routes = protect_routes(read_routes(), &auth_policy, ScopeRequirement::Read)
        .merge(protect_routes(
            write_routes(Arc::clone(&cfg), &service_context),
            &auth_policy,
            ScopeRequirement::Write,
        ))
        .merge(protect_routes(
            large_write_routes(&service_context),
            &auth_policy,
            ScopeRequirement::Write,
        ));
    Router::new()
        .route("/healthz", get(super::super::health::healthz))
        .route("/readyz", get(super::super::health::readyz))
        .route("/v1/actions", post(v1_actions_removed))
        .route("/v1/migrate", post(v1_migrate_not_exposed))
        .merge(super::openapi::docs_router())
        .merge(panel_routes())
        .merge(rest_routes)
        .fallback(super::super::static_assets::serve_static)
        .layer(middleware::from_fn(security_headers))
        .with_state((state, Arc::clone(&cfg)))
}

/// Routes reachable with `axon:read` — metadata and pure retrieval only.
fn read_routes() -> Router<ServeState> {
    Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/sources", get(handlers::discovery::sources))
        .route("/v1/domains", get(handlers::discovery::domains))
        .route("/v1/stats", get(handlers::discovery::stats))
        .route("/v1/status", get(handlers::discovery::status))
        .route("/v1/doctor", get(handlers::discovery::doctor))
        .route(
            "/v1/mobile/sessions",
            get(handlers::mobile_sessions::list_mobile_sessions),
        )
        .route(
            "/v1/mobile/sessions/{id}",
            get(handlers::mobile_sessions::get_mobile_session),
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
}

/// Routes requiring `axon:write` — active-network operations, job
/// submission, and destructive ops. Endpoint discovery fetches pages,
/// bundles, probes endpoints, and may execute Chrome capture — it must not
/// be accessible with read-only tokens.
fn write_routes(cfg: Arc<Config>, service_context: &Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .route("/v1/endpoints", post(handlers::exploration::endpoints))
        .route("/v1/brand", post(handlers::exploration::brand))
        .route("/v1/diff", post(handlers::exploration::diff))
        .route("/v1/screenshot", post(handlers::exploration::screenshot))
        .merge(ask_router::<ServeState>(cfg))
        .route("/v1/evaluate", post(handlers::rag::evaluate))
        .route("/v1/suggest", post(handlers::rag::suggest))
        .route("/v1/scrape", post(handlers::exploration::scrape))
        .route("/v1/summarize", post(handlers::exploration::summarize))
        .route(
            "/v1/summarize/stream",
            post(handlers::exploration::summarize_stream),
        )
        .route("/v1/search", post(handlers::exploration::search))
        .route("/v1/research", post(handlers::exploration::research))
        .route("/v1/memory", post(handlers::memory::memory))
        .route(
            "/v1/research/stream",
            post(handlers::exploration::research_stream),
        )
        .nest(
            "/v1/crawl",
            handlers::async_jobs::crawl_router(Arc::clone(service_context)),
        )
        .nest(
            "/v1/embed",
            handlers::async_jobs::embed_router(Arc::clone(service_context)),
        )
        .nest(
            "/v1/extract",
            handlers::async_jobs::extract_router(Arc::clone(service_context)),
        )
        .nest(
            "/v1/ingest",
            handlers::async_jobs::ingest_router(Arc::clone(service_context)),
        )
        .route("/v1/dedupe", post(handlers::admin::dedupe))
        .route(
            "/v1/watch",
            get(handlers::admin::list_watch).post(handlers::admin::create_watch),
        )
        .route("/v1/watch/{id}/run", post(handlers::admin::run_watch))
        .layer(DefaultBodyLimit::max(128 * 1024))
}

/// Write-scoped routes whose payloads exceed the standard REST body cap
/// (prepared session exports ship megabytes of transcript JSON).
fn large_write_routes(service_context: &Arc<ServiceContext>) -> Router<ServeState> {
    Router::new()
        .nest(
            "/v1/ingest/sessions/prepared",
            handlers::async_jobs::prepared_sessions_router(Arc::clone(service_context)),
        )
        .route(
            "/v1/mobile/sessions/{id}",
            put(handlers::mobile_sessions::upsert_mobile_session)
                .delete(handlers::mobile_sessions::delete_mobile_session),
        )
        .layer(DefaultBodyLimit::max(96 * 1024 * 1024))
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
        (status = 401, description = "Missing or invalid auth", body = crate::web::server::error::ErrorBody)
    ),
    tag = "discovery"
)]
pub(super) async fn v1_capabilities() -> Json<ServerInfo> {
    Json(ServerInfo::rest_capabilities())
}

async fn v1_actions_removed() -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "/v1/actions was removed; use direct /v1 REST routes",
    )
}

async fn v1_migrate_not_exposed() -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "/v1/migrate is not exposed over REST",
    )
}

pub(crate) fn ask_router<S>(cfg: Arc<Config>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::<S>::new()
        .route("/v1/ask", post(handlers::v1_ask))
        .route("/v1/ask/stream", post(handlers::v1_ask_stream))
        .route("/v1/chat", post(handlers::v1_chat))
        .route("/v1/chat/stream", post(handlers::v1_chat_stream))
        .layer(DefaultBodyLimit::max(ASK_BODY_LIMIT))
        .layer(Extension(cfg))
}

#[derive(Clone, Copy)]
pub(super) enum ScopeRequirement {
    Read,
    Write,
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
            _ => router,
        };
    };
    let router = match scope {
        ScopeRequirement::Read => router.route_layer(middleware::from_fn(require_read_scope)),
        ScopeRequirement::Write => router.route_layer(middleware::from_fn(require_write_scope)),
    };
    router
        .route_layer(layer)
        .route_layer(middleware::from_fn(normalize_api_key_header))
        .route_layer(middleware::from_fn(jsonize_auth_error))
}

async fn jsonize_auth_error(request: Request<Body>, next: middleware::Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN {
        return response;
    }
    let kind = if status == StatusCode::UNAUTHORIZED {
        "unauthorized"
    } else {
        "forbidden"
    };
    HttpError::new(status, kind, kind).into_response()
}

async fn block_loopback_destructive_request(
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    if is_loopback_destructive_request(request.method(), request.uri().path()) {
        return HttpError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "destructive REST route requires configured auth",
        )
        .into_response();
    }
    next.run(request).await
}

fn is_loopback_destructive_request(method: &Method, path: &str) -> bool {
    if *method == Method::POST
        && (path == "/v1/dedupe" || path == "/v1/watch" || path.starts_with("/v1/watch/"))
    {
        return true;
    }
    if *method == Method::POST && path == "/v1/memory" {
        return true;
    }

    for prefix in ["/v1/crawl", "/v1/embed", "/v1/extract", "/v1/ingest"] {
        if path == prefix {
            return *method == Method::POST || *method == Method::DELETE;
        }
        let Some(remainder) = path
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            continue;
        };
        if *method == Method::POST && prefix == "/v1/ingest" {
            return true;
        }
        if *method == Method::POST
            && (remainder == "cleanup" || remainder == "recover" || remainder.ends_with("/cancel"))
        {
            return true;
        }
    }
    false
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

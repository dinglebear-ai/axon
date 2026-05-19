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
use axum::{
    Extension, Router,
    body::Body,
    extract::DefaultBodyLimit,
    http::{Method, Request, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use lab_auth::AuthContext;
use std::sync::Arc;

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
    let ask_router = ask_router::<(AppState, Arc<Config>)>(Arc::clone(&cfg));
    let rest_body_limit = DefaultBodyLimit::max(128 * 1024);
    let read_routes = Router::new()
        .merge(super::super::actions::capabilities_router())
        .route("/v1/sources", get(handlers::discovery::sources))
        .route("/v1/domains", get(handlers::discovery::domains))
        .route("/v1/stats", get(handlers::discovery::stats))
        .route("/v1/status", get(handlers::discovery::status))
        .route("/v1/doctor", get(handlers::discovery::doctor))
        .route("/v1/query", post(handlers::rag::query))
        .route("/v1/retrieve", post(handlers::rag::retrieve));
    let write_routes = Router::new()
        .merge(ask_router)
        .route("/v1/evaluate", post(handlers::rag::evaluate))
        .route("/v1/suggest", post(handlers::rag::suggest))
        .route("/v1/scrape", post(handlers::exploration::scrape))
        .route("/v1/summarize", post(handlers::exploration::summarize))
        .route("/v1/map", post(handlers::exploration::map))
        .route("/v1/search", post(handlers::exploration::search))
        .route("/v1/research", post(handlers::exploration::research))
        .nest(
            "/v1/crawl",
            handlers::async_jobs::crawl_router(Arc::clone(&service_context)),
        )
        .nest(
            "/v1/embed",
            handlers::async_jobs::embed_router(Arc::clone(&service_context)),
        )
        .nest(
            "/v1/extract",
            handlers::async_jobs::extract_router(Arc::clone(&service_context)),
        )
        .nest(
            "/v1/ingest",
            handlers::async_jobs::ingest_router(Arc::clone(&service_context)),
        )
        .route("/v1/migrate", post(handlers::admin::migrate))
        .route("/v1/dedupe", post(handlers::admin::dedupe))
        .route(
            "/v1/watch",
            get(handlers::admin::list_watch).post(handlers::admin::create_watch),
        )
        .route("/v1/watch/{id}/run", post(handlers::admin::run_watch))
        .layer(rest_body_limit);
    let rest_routes = protect_routes(read_routes, &auth_policy, ScopeRequirement::Read).merge(
        protect_routes(write_routes, &auth_policy, ScopeRequirement::Write),
    );
    let panel_router = Router::new()
        .route("/healthz", get(super::super::health::healthz))
        .route("/readyz", get(super::super::health::readyz))
        .merge(super::openapi::docs_router())
        .route("/api/panel/state", get(handlers::panel_state))
        .route("/api/panel/login", post(handlers::login))
        .route(
            "/api/panel/config",
            get(handlers::get_config).put(handlers::save_config),
        )
        .route("/api/panel/ops", get(handlers::ops))
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
        .merge(rest_routes)
        .fallback(super::super::static_assets::serve_static)
        .with_state((state, Arc::clone(&cfg)));
    let v1_actions = super::super::actions::router(service_context, auth_policy.clone());
    panel_router.merge(v1_actions)
}

pub(crate) fn ask_router<S>(cfg: Arc<Config>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::<S>::new()
        .route("/v1/ask", post(handlers::v1_ask))
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
}

async fn block_loopback_destructive_request(
    request: Request<Body>,
    next: middleware::Next,
) -> Response {
    if is_loopback_destructive_request(request.method(), request.uri().path()) {
        return (
            StatusCode::UNAUTHORIZED,
            "destructive REST route requires configured auth",
        )
            .into_response();
    }
    next.run(request).await
}

fn is_loopback_destructive_request(method: &Method, path: &str) -> bool {
    if *method == Method::POST
        && (path == "/v1/dedupe"
            || path == "/v1/migrate"
            || path == "/v1/watch"
            || path.starts_with("/v1/watch/"))
    {
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
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let allowed = scope_satisfies(&auth.scopes, required_scope);
    if !allowed {
        return (
            StatusCode::FORBIDDEN,
            format!("requires scope: {required_scope}"),
        )
            .into_response();
    }
    next.run(request).await
}

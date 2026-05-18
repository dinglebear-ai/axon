use super::handlers;
use super::state::AppState;
use super::types::ASK_BODY_LIMIT;
use crate::core::config::Config;
use crate::services::context::ServiceContext;
use axum::{
    Extension, Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use std::sync::Arc;

pub(super) fn router(
    cfg: Arc<Config>,
    panel: Arc<crate::web::server::state::PanelRuntimeState>,
    service_context: Arc<tokio::sync::OnceCell<Arc<ServiceContext>>>,
    auth_policy: crate::mcp::auth::AuthPolicy,
) -> Router {
    let state = AppState {
        panel,
        service_context: Arc::clone(&service_context),
    };
    let ask_router = ask_router::<(AppState, Arc<Config>)>(Arc::clone(&cfg), &auth_policy);
    let panel_router = Router::new()
        .route("/healthz", get(super::super::health::healthz))
        .route("/readyz", get(super::super::health::readyz))
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
        .merge(ask_router)
        .fallback(super::super::static_assets::serve_static)
        .with_state((state, Arc::clone(&cfg)));
    let rest_router = handlers::rest::router(
        Arc::clone(&cfg),
        Arc::clone(&service_context),
        auth_policy.clone(),
    );
    panel_router
        .merge(super::super::actions::router(
            cfg,
            service_context,
            auth_policy,
        ))
        .merge(rest_router)
}

pub(crate) fn ask_router<S>(
    cfg: Arc<Config>,
    auth_policy: &crate::mcp::auth::AuthPolicy,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let ask_router = Router::<S>::new()
        .route("/v1/ask", post(handlers::v1_ask))
        .layer(DefaultBodyLimit::max(ASK_BODY_LIMIT))
        .layer(Extension(cfg));
    if let Some(layer) = crate::mcp::auth::build_auth_layer(
        auth_policy,
        crate::mcp::auth::configured_mcp_http_token().map(Arc::from),
        crate::mcp::auth::oauth_resource_url(auth_policy),
    ) {
        ask_router.layer(layer).layer(middleware::from_fn(
            crate::mcp::auth::normalize_api_key_header,
        ))
    } else {
        ask_router
    }
}

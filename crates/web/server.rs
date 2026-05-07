use super::auth::{PanelPassword, init_panel_password};
use crate::crates::services::query as query_svc;
use crate::crates::services::setup::{self, config_store};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct PanelRuntimeState {
    password: PanelPassword,
    setup_required: bool,
    config_path: String,
}

#[derive(Clone)]
struct AppState {
    panel: Arc<PanelRuntimeState>,
}

#[derive(Serialize)]
struct StateResponse {
    setup_required: bool,
    config_path: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    ok: bool,
    token: Option<String>,
}

#[derive(Serialize)]
struct ConfigResponse {
    path: String,
    raw_toml: String,
}

#[derive(Deserialize)]
struct SaveConfigRequest {
    raw_toml: String,
}

#[derive(Serialize)]
struct OpsResponse {
    qdrant_url: String,
    tei_url: String,
    collection: String,
    mcp_http_url: String,
}

impl PanelRuntimeState {
    pub fn initialize(host: &str, port: u16) -> std::io::Result<Self> {
        let config_init = config_store::ensure_user_config()?;
        let password_init = init_panel_password()?;
        if password_init.generated {
            eprintln!(
                "Axon web panel password: {}\nOpen: http://{}:{}",
                password_init.password.as_str(),
                host,
                port
            );
        }
        Ok(Self {
            password: password_init.password,
            setup_required: config_init.created,
            config_path: config_init.path.display().to_string(),
        })
    }

    pub fn setup_required(&self) -> bool {
        self.setup_required
    }
}

pub(crate) fn router(
    cfg: Arc<crate::crates::core::config::Config>,
    panel: Arc<PanelRuntimeState>,
) -> Router {
    let state = AppState { panel };
    Router::new()
        .route("/api/panel/state", get(panel_state))
        .route("/api/panel/login", post(login))
        .route("/api/panel/config", get(get_config).put(save_config))
        .route("/api/panel/ops", get(ops))
        .route("/api/panel/setup/targets", get(setup_targets))
        .route("/api/panel/setup/deploy", post(setup_deploy))
        .route("/v1/ask", post(v1_ask))
        .fallback(super::static_assets::serve_static)
        .with_state((state, cfg))
}

async fn panel_state(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
) -> Json<StateResponse> {
    Json(StateResponse {
        setup_required: state.panel.setup_required,
        config_path: state.panel.config_path.clone(),
    })
}

async fn login(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    if state.panel.password.verify(&req.password) {
        Json(LoginResponse {
            ok: true,
            token: Some(state.panel.password.as_str().to_string()),
        })
    } else {
        Json(LoginResponse {
            ok: false,
            token: None,
        })
    }
}

async fn get_config(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match config_store::read_config() {
        Ok(raw_toml) => Json(ConfigResponse {
            path: state.panel.config_path.clone(),
            raw_toml,
        })
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn save_config(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
    Json(req): Json<SaveConfigRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match config_store::write_config(&req.raw_toml) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn ops(
    State((state, cfg)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    Json(OpsResponse {
        qdrant_url: cfg.qdrant_url.clone(),
        tei_url: cfg.tei_url.clone(),
        collection: cfg.collection.clone(),
        mcp_http_url: format!("http://{}:{}/mcp", cfg.mcp_http_host, cfg.mcp_http_port),
    })
    .into_response()
}

async fn setup_targets(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match setup::list_ssh_targets() {
        Ok(targets) => Json(targets).into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Json(Vec::<setup::SshTarget>::new()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

async fn setup_deploy(
    State((state, _)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
    Json(req): Json<setup::DeployRequest>,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    match setup::deploy_remote(req).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

/// Request body for `POST /v1/ask`. Mirrors the subset of `Config` fields that
/// `axon ask` overrides on a per-invocation basis.
#[derive(Deserialize)]
struct AskRequestBody {
    query: String,
    #[serde(default)]
    collection: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    diagnostics: Option<bool>,
    #[serde(default)]
    graph: Option<bool>,
    #[serde(default)]
    hybrid_search: Option<bool>,
}

async fn v1_ask(
    State((_, cfg)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    if !ask_authorized(&headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if req.query.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "query is required").into_response();
    }

    let mut req_cfg = (*cfg).clone();
    if let Some(c) = req.collection {
        req_cfg.collection = c;
    }
    if let Some(s) = req.since {
        req_cfg.since = Some(s);
    }
    if let Some(b) = req.before {
        req_cfg.before = Some(b);
    }
    if let Some(d) = req.diagnostics {
        req_cfg.ask_diagnostics = d;
    }
    if let Some(g) = req.graph {
        req_cfg.ask_graph = g;
    }
    if let Some(h) = req.hybrid_search {
        req_cfg.hybrid_search_enabled = h;
    }

    match query_svc::ask(&req_cfg, &req.query, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// Authorize `/v1/ask`. When `AXON_MCP_HTTP_TOKEN` is set, require it as a
/// `Bearer` or `x-api-key` header. When unset, allow the request — matching the
/// MCP HTTP server's loopback-only convention (binding to non-loopback hosts
/// without a token is rejected at startup elsewhere).
fn ask_authorized(headers: &HeaderMap) -> bool {
    let configured = std::env::var("AXON_MCP_HTTP_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(expected) = configured else {
        return true;
    };
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    match bearer.or(api_key) {
        Some(token) => token == expected,
        None => false,
    }
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("x-axon-panel-token")
                .and_then(|v| v.to_str().ok())
        })
        .map(|token| state.panel.password.verify(token))
        .unwrap_or(false)
}

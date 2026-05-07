use super::auth::{PanelPassword, init_panel_password};
use crate::crates::services::error::diagnostics_from_error;
use crate::crates::services::query as query_svc;
use crate::crates::services::setup::{self, config_store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Hard limit on `/v1/ask` request bodies. Matches the existing 64 KiB cap used
/// by `dispatch_vector_search` so the web surface mirrors MCP behavior.
const ASK_BODY_LIMIT: usize = 64 * 1024;
/// Reject ask queries longer than this (defense-in-depth above body cap).
const ASK_QUERY_MAX_CHARS: usize = 16 * 1024;

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
        warn_if_ask_token_set_but_empty();
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
        .route(
            "/v1/ask",
            post(v1_ask).layer(DefaultBodyLimit::max(ASK_BODY_LIMIT)),
        )
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

/// Per-invocation `Config` overrides accepted by `/v1/ask`.
#[derive(Deserialize, Default)]
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
    #[serde(default)]
    ask_chunk_limit: Option<usize>,
    #[serde(default)]
    ask_full_docs: Option<usize>,
    #[serde(default)]
    ask_max_context_chars: Option<usize>,
    #[serde(default)]
    ask_hybrid_candidates: Option<usize>,
    #[serde(default)]
    ask_min_relevance_score: Option<f64>,
    #[serde(default)]
    ask_doc_chunk_limit: Option<usize>,
    #[serde(default)]
    ask_doc_fetch_concurrency: Option<usize>,
    #[serde(default)]
    ask_backfill_chunks: Option<usize>,
    #[serde(default)]
    ask_candidate_limit: Option<usize>,
    #[serde(default)]
    ask_min_citations_nontrivial: Option<usize>,
    #[serde(default)]
    ask_authoritative_domains: Option<Vec<String>>,
    #[serde(default)]
    ask_authoritative_boost: Option<f64>,
}

#[derive(Serialize)]
struct AskErrorBody {
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<serde_json::Value>,
}

/// Apply the per-request `ask_*` overrides from the body to a cloned `Config`.
fn apply_ask_overrides(req_cfg: &mut crate::crates::core::config::Config, req: &AskRequestBody) {
    if let Some(c) = req.collection.as_ref() {
        req_cfg.collection = c.clone();
    }
    if let Some(s) = req.since.as_ref() {
        req_cfg.since = Some(s.clone());
    }
    if let Some(b) = req.before.as_ref() {
        req_cfg.before = Some(b.clone());
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
    if let Some(v) = req.ask_chunk_limit {
        req_cfg.ask_chunk_limit = v;
    }
    if let Some(v) = req.ask_full_docs {
        req_cfg.ask_full_docs = v;
    }
    if let Some(v) = req.ask_max_context_chars {
        req_cfg.ask_max_context_chars = v;
    }
    if let Some(v) = req.ask_hybrid_candidates {
        req_cfg.ask_hybrid_candidates = v;
    }
    if let Some(v) = req.ask_min_relevance_score {
        req_cfg.ask_min_relevance_score = v.clamp(-1.0, 2.0);
    }
    if let Some(v) = req.ask_doc_chunk_limit {
        req_cfg.ask_doc_chunk_limit = v;
    }
    if let Some(v) = req.ask_doc_fetch_concurrency {
        req_cfg.ask_doc_fetch_concurrency = v;
    }
    if let Some(v) = req.ask_backfill_chunks {
        req_cfg.ask_backfill_chunks = v;
    }
    if let Some(v) = req.ask_candidate_limit {
        req_cfg.ask_candidate_limit = v;
    }
    if let Some(v) = req.ask_min_citations_nontrivial {
        req_cfg.ask_min_citations_nontrivial = v;
    }
    if let Some(v) = req.ask_authoritative_domains.as_ref() {
        req_cfg.ask_authoritative_domains = v.clone();
    }
    if let Some(v) = req.ask_authoritative_boost {
        req_cfg.ask_authoritative_boost = v.clamp(0.0, 10.0);
    }
}

/// Map a service error chain to (status, kind) using simple message-based
/// heuristics over the chain. Falls back to 500/internal.
fn classify_ask_error(err: &(dyn std::error::Error + 'static)) -> (StatusCode, &'static str) {
    let mut buf = String::new();
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        buf.push_str(&e.to_string());
        buf.push('\n');
        cur = e.source();
    }
    let lc = buf.to_lowercase();
    if lc.contains("query is required")
        || lc.contains("invalid collection")
        || lc.contains("invalid query")
        || lc.contains("missing required")
    {
        return (StatusCode::BAD_REQUEST, "bad_request");
    }
    if lc.contains("qdrant")
        || lc.contains("tei")
        || lc.contains("connection refused")
        || lc.contains("upstream")
        || lc.contains("timed out")
        || lc.contains("timeout")
        || lc.contains("dns")
        || lc.contains("502")
        || lc.contains("503")
    {
        return (StatusCode::BAD_GATEWAY, "upstream");
    }
    (StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

async fn v1_ask(
    State((_, cfg)): State<(AppState, Arc<crate::crates::core::config::Config>)>,
    headers: HeaderMap,
    Json(req): Json<AskRequestBody>,
) -> impl IntoResponse {
    if !ask_authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(AskErrorBody {
                kind: "unauthorized",
                message: "unauthorized".to_string(),
                diagnostics: None,
            }),
        )
            .into_response();
    }
    if req.query.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AskErrorBody {
                kind: "bad_request",
                message: "query is required".to_string(),
                diagnostics: None,
            }),
        )
            .into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(AskErrorBody {
                kind: "payload_too_large",
                message: format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"),
                diagnostics: None,
            }),
        )
            .into_response();
    }

    let mut req_cfg = (*cfg).clone();
    apply_ask_overrides(&mut req_cfg, &req);
    let want_diagnostics = req_cfg.ask_diagnostics;

    match query_svc::ask(&req_cfg, &req.query, None).await {
        Ok(result) => Json(result).into_response(),
        Err(err) => {
            let (status, kind) = classify_ask_error(err.as_ref());
            let diagnostics = if want_diagnostics {
                diagnostics_from_error(err.as_ref()).cloned()
            } else {
                None
            };
            (
                status,
                Json(AskErrorBody {
                    kind,
                    message: err.to_string(),
                    diagnostics,
                }),
            )
                .into_response()
        }
    }
}

/// Authorize `/v1/ask`. When `AXON_MCP_HTTP_TOKEN` is set, require it as a
/// `Bearer` or `x-api-key` header. When unset, allow the request — matching the
/// MCP HTTP server's loopback-only convention (binding to non-loopback hosts
/// without a token is rejected at startup elsewhere).
fn ask_authorized(headers: &HeaderMap) -> bool {
    let raw = match std::env::var("AXON_MCP_HTTP_TOKEN") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => return false,
    };
    let configured = raw.as_deref().map(str::trim).filter(|v| !v.is_empty());
    let Some(expected) = configured else {
        // Distinguish "env unset" (allow) from "env set but empty/whitespace"
        // (fail closed — operator clearly meant to enable auth).
        if raw.is_some() {
            return false;
        }
        return true;
    };
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
                .filter(|s| !s.is_empty())
        });
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    let matches_expected = |token: &str| {
        let token = token.trim();
        !token.is_empty() && bool::from(token.as_bytes().ct_eq(expected.as_bytes()))
    };
    bearer.is_some_and(matches_expected) || api_key.is_some_and(matches_expected)
}

/// Log a startup warning when `AXON_MCP_HTTP_TOKEN` is set but resolves to
/// empty/whitespace — the operator clearly meant to enable auth, and
/// `ask_authorized` will fail closed in that case.
pub(crate) fn warn_if_ask_token_set_but_empty() {
    if let Ok(raw) = std::env::var("AXON_MCP_HTTP_TOKEN")
        && !raw.is_empty()
        && raw.trim().is_empty()
    {
        tracing::warn!(
            context = "v1_ask_startup",
            "AXON_MCP_HTTP_TOKEN is set to whitespace \u{2014} /v1/ask will reject all requests"
        );
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

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;

pub(crate) mod cors;
mod docker_stats;
mod download;
pub mod execute;
mod pack;
mod shell;
pub mod tailscale_auth;
mod ws_handler;

use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use dashmap::DashMap;
use serde::Deserialize;
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tailscale_auth::{AuthOutcome, DenyReason, auth_log_message, check_auth};
use tokio::sync::broadcast;
use uuid::Uuid;

use cors::{effective_shell_allowed_origins, web_cors_middleware, websocket_origin_is_allowed};

/// Global semaphore limiting concurrent ACP sessions (pulse_chat + pulse_chat_probe).
/// Prevents unbounded `spawn_blocking` thread consumption — each ACP session holds a
/// thread for up to 300 seconds.  Read from `AXON_ACP_MAX_CONCURRENT_SESSIONS` env var;
/// default 8.  (SEC-8 / PERF-1 / PERF-10)
pub(crate) static ACP_SESSION_SEMAPHORE: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| {
        let max = std::env::var("AXON_ACP_MAX_CONCURRENT_SESSIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(8);
        tokio::sync::Semaphore::new(max)
    });

// Connection limit infrastructure for L-7/L-12.
static WS_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);
fn max_ws_connections() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("AXON_MAX_WS_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100)
    })
}
static SHELL_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);
fn max_shell_connections() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("AXON_MAX_SHELL_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10)
    })
}

/// Atomically acquire a connection slot if under `limit`.
///
/// Uses `fetch_update` (CAS loop) so the check-and-increment is a single
/// atomic operation — no TOCTOU race between separate `load()` and `fetch_add()`.
fn try_acquire_connection(counter: &'static AtomicUsize, limit: usize) -> bool {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
            (current < limit).then_some(current + 1)
        })
        .is_ok()
}

struct ConnectionGuard {
    counter: &'static AtomicUsize,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Release);
    }
}

/// Shared state across all WS connections.
pub(crate) struct AppState {
    /// Docker stats broadcast — poller sends, every WS client subscribes.
    stats_tx: broadcast::Sender<String>,
    /// Registry of completed job IDs → output directories for download routes.
    job_dirs: Arc<DashMap<String, PathBuf>>,
    /// Static API token for the WS gate. Set from AXON_WEB_API_TOKEN.
    /// None = gate disabled (open WS, trusted-network deployments only).
    api_token: Option<String>,
    /// Base server config — shared across all connections.
    pub(crate) cfg: Arc<Config>,
    /// Maps ACP session_id → conn_id that originally resumed it (H-8).
    /// Prevents cross-connection session hijacking: only the originating
    /// connection may drain replay buffers or route permission responses.
    pub(crate) session_ownership: Arc<DashMap<String, String>>,
    /// Process-wide rate limiter keyed by client IP (P1-2).
    /// Value is (exec_count, exec_window, read_count, read_window) — each category
    /// has its own independent sliding window to prevent cross-category reset exploits.
    /// Prevents bypass via WebSocket reconnect — state survives across connections.
    pub(crate) rate_limiter:
        Arc<DashMap<std::net::IpAddr, (u32, std::time::Instant, u32, std::time::Instant)>>,
}

/// State for download routes — lighter than AppState (no WS/stats fields).
pub(crate) struct DownloadAuthState {
    pub job_dirs: Arc<DashMap<String, PathBuf>>,
    pub api_token: Option<String>,
}

/// Query parameters for the `/ws` upgrade request.
#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

// ── Server startup ────────────────────────────────────────────────────────────

pub async fn start_server(port: u16, cfg: Arc<Config>) -> Result<(), Box<dyn Error>> {
    let (stats_tx, _) = broadcast::channel::<String>(64);
    let job_dirs: Arc<DashMap<String, PathBuf>> = Arc::new(DashMap::new());

    let api_token = std::env::var("AXON_WEB_API_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());

    match &api_token {
        Some(_) => log_info("WS gate: api token"),
        None => log_info("WS gate: open in debug/test builds; set AXON_WEB_API_TOKEN for auth"),
    }

    let state = Arc::new(AppState {
        stats_tx: stats_tx.clone(),
        job_dirs: job_dirs.clone(),
        api_token: api_token.clone(),
        cfg: cfg.clone(),
        session_ownership: Arc::new(DashMap::new()),
        rate_limiter: Arc::new(DashMap::new()),
    });

    // P2-1: Wrap docker stats in a restart loop so a failure or unexpected
    // return does not permanently kill stats broadcasting for all WS clients.
    tokio::spawn({
        let stats_tx_restart = stats_tx.clone();
        async move {
            loop {
                docker_stats::run_stats_loop(stats_tx_restart.clone()).await;
                // run_stats_loop returns () — if it exits, something went wrong
                // (Docker unavailable, channel closed, etc.).  Restart after a delay.
                log_warn("docker stats loop exited unexpectedly; restarting in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    });

    let download_state = Arc::new(DownloadAuthState {
        job_dirs: job_dirs.clone(),
        api_token,
    });

    let download_routes = Router::new()
        .route("/download/{job_id}/pack.md", get(download::serve_pack_md))
        .route("/download/{job_id}/pack.xml", get(download::serve_pack_xml))
        .route("/download/{job_id}/archive.zip", get(download::serve_zip))
        .route("/download/{job_id}/file/{*path}", get(download::serve_file))
        .with_state(download_state);

    let app = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/ws/shell", get(shell_ws_upgrade))
        .route("/output/{*path}", get(serve_output_file))
        .with_state(state)
        .merge(download_routes)
        .layer(middleware::from_fn_with_state(
            cfg.clone(),
            web_cors_middleware,
        ));

    let host = std::env::var("AXON_SERVE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    log_info(&format!(
        "Axon web UI starting with AXON_SERVE_HOST={} port={}",
        host, port
    ));
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)));

    log_info(&format!("Axon web UI listening on http://{addr}"));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    match tokio::signal::ctrl_c().await {
        Ok(()) => {}
        Err(e) => {
            log_warn(&format!(
                "ctrl+c handler failed: {e}; server requires SIGKILL to stop"
            ));
            std::future::pending::<()>().await;
        }
    }
}

// ── Auth helper ───────────────────────────────────────────────────────────────

fn http_auth(
    req_headers: &HeaderMap,
    query_token: Option<&str>,
    api_token: Option<&str>,
) -> AuthOutcome {
    check_auth(req_headers, query_token, api_token)
}

// ── Output file serving ───────────────────────────────────────────────────────

async fn serve_output_file(
    axum::extract::Path(file_path): axum::extract::Path<String>,
    req_headers: HeaderMap,
    Query(params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    use axum::http::header;

    let auth = http_auth(
        &req_headers,
        params.token.as_deref(),
        state.api_token.as_deref(),
    );
    if matches!(auth, AuthOutcome::Denied(_)) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    // Defense-in-depth: quick pre-check before canonicalize — does not replace canonicalization below.
    // Uses Path::components() instead of substring matching so that valid filenames
    // containing ".." (e.g. "report..json") are not incorrectly rejected.
    if std::path::Path::new(&file_path)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
        || file_path.contains('\0')
    {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let base = execute::files::output_dir();
    let full_path = base.join(&file_path);

    let Ok(canonical_base) = tokio::fs::canonicalize(&base).await else {
        return (StatusCode::NOT_FOUND, "output directory not found").into_response();
    };
    let Ok(canonical_file) = tokio::fs::canonicalize(&full_path).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };

    if !canonical_file.starts_with(&canonical_base) {
        return (StatusCode::FORBIDDEN, "path outside output directory").into_response();
    }

    let bytes = match tokio::fs::read(&canonical_file).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::NOT_FOUND, "file not found").into_response(),
    };

    let content_type = match canonical_file.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("md") => "text/markdown; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type),
    );
    resp_headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=300"),
    );

    (resp_headers, bytes).into_response()
}

// ── WebSocket upgrade handlers ────────────────────────────────────────────────

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !websocket_origin_is_allowed(&headers, &state.cfg.web_allowed_origins) {
        return (StatusCode::FORBIDDEN, "forbidden: origin not allowed").into_response();
    }

    // L-7: enforce WS connection limit to prevent file descriptor exhaustion.
    // Uses atomic CAS via try_acquire_connection to avoid TOCTOU race between
    // separate load() + fetch_add() calls (Thread 10).
    if !try_acquire_connection(&WS_CONNECTION_COUNT, max_ws_connections()) {
        log::warn!(
            "[ws] connection limit reached ({}/{}), rejecting upgrade from {}",
            WS_CONNECTION_COUNT.load(Ordering::Relaxed),
            max_ws_connections(),
            addr.ip()
        );
        return (StatusCode::SERVICE_UNAVAILABLE, "too many connections").into_response();
    }
    // Pre-acquired a slot; if auth fails or upgrade never fires we must release it.
    // The ConnectionGuard handles decrement on drop in all paths.
    let pre_guard = ConnectionGuard {
        counter: &WS_CONNECTION_COUNT,
    };

    let outcome = http_auth(
        &headers,
        params.token.as_deref(),
        state.api_token.as_deref(),
    );

    let log_msg = auth_log_message(&outcome, addr);
    match &outcome {
        AuthOutcome::Token => log::info!("{log_msg}"),
        AuthOutcome::Denied(_) => log::warn!("{log_msg}"),
    }

    if matches!(outcome, AuthOutcome::Denied(_)) {
        // pre_guard drops here, releasing the slot
        drop(pre_guard);
        let body = match outcome {
            AuthOutcome::Denied(DenyReason::NoAuthConfigured) => {
                "unauthorized: configure AXON_WEB_API_TOKEN"
            }
            _ => "unauthorized",
        };
        return (StatusCode::UNAUTHORIZED, body).into_response();
    }

    let conn_id = Uuid::new_v4().to_string();
    let client_ip = addr.ip();
    // Move the guard into the on_upgrade callback so it lives for the
    // duration of the WS connection. If the HTTP upgrade handshake fails
    // (client disconnects), axum drops the future — the guard drops with
    // it, releasing the slot. (Thread 21)
    ws.on_upgrade(move |socket| {
        let _guard = pre_guard;
        async move {
            ws_handler::handle_ws(socket, state, conn_id, client_ip).await;
            drop(_guard);
        }
    })
}

async fn shell_ws_upgrade(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let allowed_origins = effective_shell_allowed_origins(
        &state.cfg.shell_allowed_origins,
        &state.cfg.web_allowed_origins,
    );
    if !websocket_origin_is_allowed(&headers, allowed_origins) {
        return (StatusCode::FORBIDDEN, "forbidden: shell origin not allowed").into_response();
    }

    // L-12: enforce shell WS connection limit.
    // Uses atomic CAS via try_acquire_connection — same pattern as ws_upgrade (Thread 10/20).
    if !try_acquire_connection(&SHELL_CONNECTION_COUNT, max_shell_connections()) {
        log::warn!(
            "[shell ws] connection limit reached ({}/{}), rejecting upgrade from {}",
            SHELL_CONNECTION_COUNT.load(Ordering::Relaxed),
            max_shell_connections(),
            addr.ip()
        );
        return (StatusCode::SERVICE_UNAVAILABLE, "too many connections").into_response();
    }
    let pre_guard = ConnectionGuard {
        counter: &SHELL_CONNECTION_COUNT,
    };

    let outcome = http_auth(
        &headers,
        params.token.as_deref(),
        state.api_token.as_deref(),
    );
    let log_msg = auth_log_message(&outcome, addr);
    match &outcome {
        AuthOutcome::Token => log::info!("shell ws: {log_msg}"),
        AuthOutcome::Denied(_) => log::warn!("shell ws: {log_msg}"),
    }
    if matches!(outcome, AuthOutcome::Denied(_)) {
        drop(pre_guard);
        return (
            StatusCode::FORBIDDEN,
            "shell access denied — set AXON_WEB_API_TOKEN",
        )
            .into_response();
    }

    // Q92M: cap inbound message size at the upgrade level so axum rejects
    // oversized frames before allocating them — the per-message check inside
    // handle_shell_ws still guards PTY writes but is no longer the sole
    // allocation defence.
    const MAX_SHELL_WS_MSG: usize = 65_536; // 64 KiB
    // Move guard into the on_upgrade callback — if the HTTP upgrade handshake
    // fails, axum drops the future and the guard releases the slot. (Thread 20)
    ws.max_message_size(MAX_SHELL_WS_MSG)
        .on_upgrade(move |socket| {
            let _guard = pre_guard;
            async move {
                shell::handle_shell_ws(socket).await;
                drop(_guard);
            }
        })
}

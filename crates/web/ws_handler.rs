//! WebSocket connection state and message dispatch.
//!
//! Extracted from `web.rs` to stay under the 500-line module limit.
//! Contains `WsConnState`, the WS read/forward loops, and message routing
//! (execute, cancel, permission_response, read_file, acp_resume).

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::services::acp::{PermissionResponderMap, SESSION_CACHE};

use super::AppState;
use super::execute;
use super::execute::events::{CommandContext, CommandErrorPayload, WsEventV2, serialize_v2_event};

/// Maximum `execute` messages per IP per 60-second window (H-12, P1-2).
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_MAX_EXECUTES: u32 = 120;
/// Maximum `read_file` messages per IP per 60-second window (P3-4).
const RATE_LIMIT_MAX_READ_FILE: u32 = 60;

/// Incoming WS message from the browser.
#[derive(Deserialize)]
struct WsClientMsg {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    flags: serde_json::Value,
    #[serde(default, alias = "exec_id")]
    id: String,
    #[serde(default)]
    path: String,
    /// Permission response: the tool_call_id being responded to.
    #[serde(default)]
    tool_call_id: String,
    /// Permission response: the chosen option_id.
    #[serde(default)]
    option_id: String,
    /// Session ID context for acp_resume / permission_response validation.
    #[serde(default)]
    session_id: String,
}

/// Lightweight probe for the `"type"` field — avoids substring scanning (P1-8).
#[derive(Deserialize)]
struct MsgType<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
}

/// Per-connection state shared across the read loop and spawned tasks.
struct WsConnState {
    exec_tx: mpsc::Sender<String>,
    tracking_tx: mpsc::Sender<String>,
    crawl_job_id: Arc<Mutex<Option<String>>>,
    crawl_base_dir: Arc<Mutex<Option<PathBuf>>>,
    permission_responders: PermissionResponderMap,
    conn_cfg: Arc<Config>,
    /// Unique ID for this WS connection — used for session ownership checks (H-8).
    conn_id: String,
    /// Process-wide map of session_id → conn_id that claimed it (H-8).
    session_ownership: Arc<DashMap<String, String>>,
    /// Client IP for process-wide rate limiting (P1-2).
    client_ip: IpAddr,
    /// Process-wide rate limiter — `(execute_count, read_file_count, window_start)`.
    rate_limiter: Arc<DashMap<IpAddr, (u32, u32, Instant)>>,
}

/// Create a fresh `PermissionResponderMap` for a new WS connection.
fn init_permission_responders() -> PermissionResponderMap {
    Arc::new(DashMap::new())
}

/// Which rate-limit counter to check/increment.
enum RateLimitCategory {
    Execute,
    ReadFile,
}

/// Main WS connection handler — runs the read/forward loops until disconnect.
pub(super) async fn handle_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    conn_id: String,
    client_ip: IpAddr,
) {
    let (ws_tx, mut ws_rx) = socket.split();

    let (exec_tx, exec_rx) = mpsc::channel::<String>(256);
    let (tracking_tx, tracking_rx) = mpsc::channel::<String>(256);

    let crawl_base_dir: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
    let crawl_job_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let permission_responders = init_permission_responders();

    let stats_rx = state.stats_tx.subscribe();

    // Forward task: sends exec output + stats to the WS client (P2-5, P3-1).
    let base_dir_tracker = crawl_base_dir.clone();
    let job_dirs_tracker = state.job_dirs.clone();
    let forward = tokio::spawn(run_forward_task(
        exec_rx,
        tracking_rx,
        stats_rx,
        base_dir_tracker,
        job_dirs_tracker,
        ws_tx,
    ));

    let conn = WsConnState {
        exec_tx,
        tracking_tx,
        crawl_job_id,
        crawl_base_dir,
        permission_responders,
        conn_cfg: state.cfg.clone(),
        conn_id,
        session_ownership: state.session_ownership.clone(),
        client_ip,
        rate_limiter: state.rate_limiter.clone(),
    };

    let mut tasks: JoinSet<()> = JoinSet::new();

    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg {
            let Ok(client_msg) = serde_json::from_str::<WsClientMsg>(&text) else {
                let _ = conn
                    .exec_tx
                    .send(r#"{"type":"error","message":"invalid JSON"}"#.to_string())
                    .await;
                continue;
            };
            handle_ws_message(&conn, client_msg, &mut tasks).await;
        }
    }

    // P1-1: shut down all spawned tasks before tearing down the connection.
    tasks.shutdown().await;

    // P1-3: remove session ownership entries for this connection.
    let cid = &conn.conn_id;
    conn.session_ownership
        .retain(|_, owner| owner.as_str() != cid.as_str());

    forward.abort();
}

/// Forward task body: drains exec output, tracking, and stats channels to the WS
/// sink. Uses `biased` select to prioritize output over stats (P3-1).
/// Detects `crawl_files` messages via proper type-field parse (P1-8).
async fn run_forward_task(
    mut exec_rx: mpsc::Receiver<String>,
    mut tracking_rx: mpsc::Receiver<String>,
    mut stats_rx: tokio::sync::broadcast::Receiver<String>,
    base_dir_tracker: Arc<Mutex<Option<PathBuf>>>,
    job_dirs_tracker: Arc<DashMap<String, PathBuf>>,
    mut ws_tx: impl SinkExt<Message> + Unpin,
) {
    loop {
        tokio::select! {
            biased;
            Some(msg) = exec_rx.recv() => {
                track_crawl_files(&msg, &base_dir_tracker, &job_dirs_tracker).await;
                if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Some(msg) = tracking_rx.recv() => {
                if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Ok(stats_msg) = stats_rx.recv() => {
                if ws_tx.send(Message::Text(stats_msg.into())).await.is_err() {
                    break;
                }
            }
            else => break,
        }
    }
}

/// Detect `crawl_files` messages by their `"type"` field (P1-8) and track
/// `output_dir` / `job_id` for the crawl base dir and job dirs registries.
async fn track_crawl_files(
    msg: &str,
    base_dir: &Mutex<Option<PathBuf>>,
    job_dirs: &DashMap<String, PathBuf>,
) {
    let is_crawl_files = serde_json::from_str::<MsgType>(msg)
        .map(|m| m.msg_type == "crawl_files")
        .unwrap_or(false);
    if !is_crawl_files {
        return;
    }
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(msg) else {
        return;
    };
    if let Some(dir) = parsed.get("output_dir").and_then(|v| v.as_str()) {
        *base_dir.lock().await = Some(PathBuf::from(dir));
    }
    if let (Some(job_id), Some(dir)) = (
        parsed.get("job_id").and_then(|v| v.as_str()),
        parsed.get("output_dir").and_then(|v| v.as_str()),
    ) {
        job_dirs.insert(job_id.to_string(), PathBuf::from(dir));
    }
}

/// Check the process-wide rate limiter for a given message category (P1-2, P3-4).
/// Returns `true` if the request is allowed, `false` if rate-limited.
fn check_rate_limit(
    rate_limiter: &DashMap<IpAddr, (u32, u32, Instant)>,
    ip: IpAddr,
    category: RateLimitCategory,
) -> bool {
    let now = Instant::now();
    let window = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
    let mut entry = rate_limiter.entry(ip).or_insert((0, 0, now));
    let (exec_count, read_count, window_start) = entry.value_mut();

    if now.duration_since(*window_start) > window {
        *exec_count = 0;
        *read_count = 0;
        *window_start = now;
    }

    match category {
        RateLimitCategory::Execute => {
            *exec_count += 1;
            *exec_count <= RATE_LIMIT_MAX_EXECUTES
        }
        RateLimitCategory::ReadFile => {
            *read_count += 1;
            *read_count <= RATE_LIMIT_MAX_READ_FILE
        }
    }
}

/// Dispatch a single parsed WS client message to the appropriate handler.
async fn handle_ws_message(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    match client_msg.msg_type.as_str() {
        "execute" => handle_execute_msg(conn, client_msg, tasks).await,
        "acp_resume" => handle_acp_resume(conn, &client_msg.session_id).await,
        "cancel" => {
            let tx = conn.exec_tx.clone();
            let job_id_arc = conn.crawl_job_id.clone();
            let cancel_mode = client_msg.mode;
            let cancel_cfg = conn.conn_cfg.clone();
            let cancel_id = client_msg.id;
            tasks.spawn(async move {
                let stored = job_id_arc.lock().await.clone();
                let id = stored.or(if cancel_id.is_empty() {
                    None
                } else {
                    Some(cancel_id)
                });
                if let Some(id) = id {
                    execute::handle_cancel(&cancel_mode, &id, tx, cancel_cfg).await;
                }
            });
        }
        "permission_response" => {
            route_permission_response(
                conn,
                client_msg.tool_call_id,
                client_msg.option_id,
                client_msg.session_id,
            );
        }
        "read_file" => handle_read_file_msg(conn, client_msg, tasks).await,
        _ => {}
    }
}

/// Handle an `execute` message with process-wide rate limiting (P1-2, P1-9, P2-2).
async fn handle_execute_msg(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    if !check_rate_limit(
        &conn.rate_limiter,
        conn.client_ip,
        RateLimitCategory::Execute,
    ) {
        let ctx = CommandContext {
            exec_id: client_msg.id.clone(),
            mode: client_msg.mode.clone(),
            input: client_msg.input.clone(),
        };
        let event = WsEventV2::CommandError {
            ctx,
            payload: CommandErrorPayload {
                message: "rate limit exceeded".into(),
                elapsed_ms: None,
            },
        };
        if let Some(json) = serialize_v2_event(event) {
            let _ = conn.exec_tx.send(json).await;
        }
        return;
    }

    let exec_id = if client_msg.id.trim().is_empty() {
        format!("exec-{}", Uuid::new_v4())
    } else {
        client_msg.id
    };
    let context = execute::ExecCommandContext {
        exec_id,
        mode: client_msg.mode,
        input: client_msg.input,
        flags: client_msg.flags,
        cfg: conn.conn_cfg.clone(),
    };

    let tx = conn.exec_tx.clone();
    let job_id = conn.crawl_job_id.clone();
    let perm_map = conn.permission_responders.clone();
    tasks.spawn(async move {
        execute::handle_command(context, tx, job_id, perm_map).await;
    });
}

/// Handle a `read_file` message with rate limiting (P3-4).
async fn handle_read_file_msg(
    conn: &WsConnState,
    client_msg: WsClientMsg,
    tasks: &mut JoinSet<()>,
) {
    if client_msg.path.is_empty() {
        return;
    }

    if !check_rate_limit(
        &conn.rate_limiter,
        conn.client_ip,
        RateLimitCategory::ReadFile,
    ) {
        let _ = conn
            .tracking_tx
            .send(r#"{"type":"error","message":"read_file rate limit exceeded"}"#.to_string())
            .await;
        return;
    }

    let tx = conn.tracking_tx.clone();
    let path = client_msg.path;
    let base = conn.crawl_base_dir.clone();
    tasks.spawn(async move {
        let base_dir_opt: Option<PathBuf> = base.lock().await.clone();
        if let Some(base_dir) = base_dir_opt {
            execute::handle_read_file(&path, &base_dir, tx).await;
        } else {
            let _ = tx
                .send(
                    serde_json::json!({"type":"error","message":"no crawl output available"})
                        .to_string(),
                )
                .await;
        }
    });
}

/// Handle `acp_resume` — reconnect to a cached ACP session and replay buffered
/// events.
///
/// Security (H-8): records `conn_id` as the owner of this `session_id` on first
/// resume. Subsequent resume attempts from a different connection are rejected.
async fn handle_acp_resume(conn: &WsConnState, session_id: &str) {
    let tx = &conn.exec_tx;

    if session_id.is_empty() {
        let _ = tx
            .send(
                serde_json::json!({
                    "type": "acp_resume_result",
                    "ok": false,
                    "reason": "missing session_id"
                })
                .to_string(),
            )
            .await;
        return;
    }

    let Some(cached) = SESSION_CACHE.get_by_session_id(session_id) else {
        let _ = tx
            .send(
                serde_json::json!({
                    "type": "acp_resume_result",
                    "ok": false,
                    "reason": "session not found",
                    "session_id": session_id
                })
                .to_string(),
            )
            .await;
        log::info!("[ws] acp_resume: session_id={session_id} not found in cache");
        return;
    };

    // H-8: enforce connection-binding.
    match conn
        .session_ownership
        .entry(session_id.to_string())
        .or_insert_with(|| conn.conn_id.clone())
        .value()
        .clone()
    {
        ref owner if owner != &conn.conn_id => {
            log::warn!(
                "[ws] acp_resume denied: session_id={session_id} bound to different connection"
            );
            let _ = tx
                .send(
                    serde_json::json!({
                        "type": "acp_resume_result",
                        "ok": false,
                        "reason": "session bound to another connection"
                    })
                    .to_string(),
                )
                .await;
            return;
        }
        _ => {}
    }

    let buffered = cached.drain_replay_buffer();
    let replayed = buffered.len();
    for msg in buffered {
        let _ = tx.send(msg).await;
    }
    let _ = tx
        .send(
            serde_json::json!({
                "type": "acp_resume_result",
                "ok": true,
                "session_id": session_id,
                "replayed": replayed
            })
            .to_string(),
        )
        .await;
    log::info!("[ws] acp_resume: session_id={session_id}, replayed {replayed} buffered event(s)");
}

/// Route a `permission_response` message to the waiting ACP session.
///
/// Security (H-8): for resumed sessions, validates that the requesting
/// connection owns the session before routing.
fn route_permission_response(
    conn: &WsConnState,
    tool_call_id: String,
    option_id: String,
    session_id: String,
) {
    if tool_call_id.is_empty() || option_id.is_empty() {
        log::warn!("permission_response with empty tool_call_id or option_id — ignoring");
        return;
    }
    log::debug!("permission_response: session_id={session_id} tool_call_id={tool_call_id}");

    if let Some((_, sender)) = conn
        .permission_responders
        .remove(&(session_id.clone(), tool_call_id.clone()))
    {
        let _ = sender.send(option_id);
        return;
    }

    let owned_by_this_conn = conn
        .session_ownership
        .get(&session_id)
        .is_some_and(|owner| *owner == conn.conn_id);

    if !owned_by_this_conn {
        log::warn!(
            "permission_response denied: session_id={session_id} not owned by this connection \
             (tool_call_id={tool_call_id})"
        );
        return;
    }

    if let Some(cached) = SESSION_CACHE.get_by_session_id_sync(&session_id)
        && let Some((_, sender)) = cached
            .permission_responders
            .remove(&(session_id.clone(), tool_call_id.clone()))
    {
        let _ = sender.send(option_id);
        return;
    }

    log::warn!(
        "permission_response for unknown key: session_id={session_id} \
         tool_call_id={tool_call_id} (already responded or wrong session)"
    );
}

#[cfg(test)]
mod tests;

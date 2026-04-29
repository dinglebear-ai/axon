//! WebSocket connection state and message dispatch.
//!
//! Contains `WsConnState`, the WS read/forward loops, and message routing
//! (execute, cancel, permission_response, read_file, acp_resume).

mod acp_session;
mod rate_limiter;

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::services::acp::PermissionResponderMap;
use crate::crates::services::context::ServiceContext;
#[cfg(test)]
use acp_session::acp_resume_json;
#[cfg(test)]
use rate_limiter::{RATE_LIMIT_MAX_EXECUTES, RATE_LIMIT_MAX_READ_FILE, RATE_LIMIT_WINDOW_SECS};
use rate_limiter::{RateLimitCategory, check_rate_limit};

use super::AppState;
use super::execute;
use super::execute::events::{CommandContext, CommandErrorPayload, WsEventV2, serialize_v2_event};

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
#[cfg(test)]
#[derive(Deserialize)]
struct MsgType<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
}

/// Per-connection state shared across the read loop and spawned tasks.
struct WsConnState {
    exec_tx: mpsc::Sender<String>,
    tracking_tx: mpsc::Sender<String>,
    /// M-5: Cumulative list of all enqueued job IDs for this connection.
    /// Implicit cancel (no explicit job_id) iterates and cancels all entries.
    /// Each execute task creates its own per-task `Arc<Mutex<Option<String>>>` for
    /// `handle_command`, then propagates the stored ID here after completion.
    crawl_job_ids: Arc<Mutex<Vec<String>>>,
    crawl_base_dir: Arc<Mutex<Option<PathBuf>>>,
    permission_responders: PermissionResponderMap,
    conn_cfg: Arc<Config>,
    service_context: Arc<ServiceContext>,
    conn_id: String,
    session_ownership: Arc<DashMap<String, String>>,
    client_ip: IpAddr,
    rate_limiter: Arc<DashMap<IpAddr, (u32, Instant, u32, Instant)>>,
    /// M-12: Whether this client has opted in to Docker stats broadcasts.
    stats_subscribed: Arc<AtomicBool>,
}

fn init_permission_responders() -> PermissionResponderMap {
    Arc::new(DashMap::new())
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
    let crawl_job_ids: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let permission_responders = init_permission_responders();
    let stats_subscribed = Arc::new(AtomicBool::new(false));

    let stats_rx = state.stats_tx.subscribe();

    // Forward task: sends exec output + stats to the WS client (P2-5, P3-1).
    let base_dir_tracker = crawl_base_dir.clone();
    let job_dirs_tracker = state.job_dirs.clone();
    let stats_sub_flag = stats_subscribed.clone();
    let forward = tokio::spawn(run_forward_task(
        exec_rx,
        tracking_rx,
        stats_rx,
        base_dir_tracker,
        job_dirs_tracker,
        stats_sub_flag,
        ws_tx,
    ));

    let conn = WsConnState {
        exec_tx,
        tracking_tx,
        crawl_job_ids,
        crawl_base_dir,
        permission_responders,
        conn_cfg: state.cfg.clone(),
        service_context: state.service_context.clone(),
        conn_id,
        session_ownership: state.session_ownership.clone(),
        client_ip,
        rate_limiter: state.rate_limiter.clone(),
        stats_subscribed,
    };

    let mut tasks: JoinSet<()> = JoinSet::new();

    tracing::info!(conn_id = %conn.conn_id, client_ip = %conn.client_ip, "ws: connection opened");
    let _ws_open_time = Instant::now();

    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg {
            let Ok(client_msg) = serde_json::from_str::<WsClientMsg>(&text) else {
                tracing::warn!(
                    conn_id = %conn.conn_id,
                    "ws: invalid JSON frame received"
                );
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
    tracing::info!(
        conn_id = %conn.conn_id,
        client_ip = %conn.client_ip,
        duration_ms = %_ws_open_time.elapsed().as_millis(),
        "ws: connection closed",
    );

    // P1-3: remove session ownership entries for this connection.
    let cid = &conn.conn_id;
    conn.session_ownership
        .retain(|_, owner| owner.as_str() != cid.as_str());

    forward.abort();
}

/// Forward task body: drains exec output, tracking, and stats channels to the WS
/// sink. Uses `biased` select to prioritize output over stats (M-16, P3-1).
/// Stats are only forwarded when the client has opted in (M-12).
async fn run_forward_task(
    mut exec_rx: mpsc::Receiver<String>,
    mut tracking_rx: mpsc::Receiver<String>,
    mut stats_rx: tokio::sync::broadcast::Receiver<String>,
    base_dir_tracker: Arc<Mutex<Option<PathBuf>>>,
    job_dirs_tracker: Arc<DashMap<String, PathBuf>>,
    stats_subscribed: Arc<AtomicBool>,
    mut ws_tx: impl SinkExt<Message> + Unpin,
) {
    loop {
        // M-16: `biased;` guarantees deterministic poll order:
        //   1. exec output (highest — user-visible command results)
        //   2. tracking (mid — read_file responses)
        //   3. stats (lowest — acceptable to starve during heavy crawl output)
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
                // M-12: only forward stats when the client has subscribed.
                if stats_subscribed.load(Ordering::Relaxed)
                    && ws_tx.send(Message::Text(stats_msg.into())).await.is_err()
                {
                    break;
                }
            }
            else => break,
        }
    }
}

/// Detect `crawl_files` messages by their `"type"` field and track
/// `output_dir` / `job_id` for the crawl base dir and job dirs registries.
///
/// H5: parse once into `serde_json::Value`, then check `["type"]` and extract
/// fields from the same parsed tree — eliminates the double JSON parse.
async fn track_crawl_files(
    msg: &str,
    base_dir: &Mutex<Option<PathBuf>>,
    job_dirs: &DashMap<String, PathBuf>,
) {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(msg) else {
        return;
    };
    let is_crawl_files = parsed
        .get("type")
        .and_then(|v| v.as_str())
        .is_some_and(|t| t == "crawl_files");
    if !is_crawl_files {
        return;
    }
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

/// Dispatch a single parsed WS client message to the appropriate handler.
async fn handle_ws_message(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    match client_msg.msg_type.as_str() {
        "execute" => handle_execute_msg(conn, client_msg, tasks).await,
        "acp_resume" => acp_session::handle_acp_resume(conn, &client_msg.session_id).await,
        "cancel" => handle_cancel_msg(conn, client_msg, tasks),
        "permission_response" => {
            acp_session::route_permission_response(
                conn,
                client_msg.tool_call_id,
                client_msg.option_id,
                client_msg.session_id,
            );
        }
        "read_file" => handle_read_file_msg(conn, client_msg, tasks).await,
        // M-12: opt-in / opt-out for Docker stats broadcasting.
        "subscribe_stats" => {
            conn.stats_subscribed.store(true, Ordering::Relaxed);
        }
        "unsubscribe_stats" => {
            conn.stats_subscribed.store(false, Ordering::Relaxed);
        }
        _ => {
            tracing::warn!(
                conn_id = %conn.conn_id,
                "ws: unknown message type discarded"
            );
        }
    }
}

/// M-5: Handle a `cancel` message — cancels an explicit job ID or ALL tracked jobs.
///
/// When the client sends an explicit `id`, only that job is canceled and trimmed
/// from the tracking Vec. When `id` is empty (implicit cancel), every job ID in
/// `crawl_job_ids` is canceled so that multi-crawl sessions do not silently leave
/// older jobs running.
fn handle_cancel_msg(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    let tx = conn.exec_tx.clone();
    let job_ids_arc = conn.crawl_job_ids.clone();
    let cancel_mode = client_msg.mode;
    let cancel_service_context = conn.service_context.clone();
    let cancel_id = client_msg.id;

    tasks.spawn(async move {
        if !cancel_id.is_empty() {
            // Explicit job ID — cancel only that one.
            execute::handle_cancel(&cancel_mode, &cancel_id, tx, cancel_service_context).await;
            job_ids_arc.lock().await.retain(|id| id != &cancel_id);
        } else {
            // Implicit cancel — cancel ALL tracked job IDs.
            let ids: Vec<String> = job_ids_arc.lock().await.clone();
            for id in &ids {
                execute::handle_cancel(
                    &cancel_mode,
                    id,
                    tx.clone(),
                    cancel_service_context.clone(),
                )
                .await;
            }
            job_ids_arc.lock().await.clear();
        }
    });
}

/// Handle an `execute` message with process-wide rate limiting (P1-2, P1-9, P2-2).
async fn handle_execute_msg(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    if !check_rate_limit(
        &conn.rate_limiter,
        conn.client_ip,
        RateLimitCategory::Execute,
    ) {
        tracing::warn!(
            conn_id = %conn.conn_id,
            client_ip = %conn.client_ip,
            category = "execute",
            "ws: rate limit exceeded"
        );
        let ctx = CommandContext {
            exec_id: client_msg.id.clone(),
            mode: client_msg.mode.clone(),
            input: client_msg.input.clone(),
        };
        let event = WsEventV2::CommandError {
            ctx,
            payload: CommandErrorPayload {
                message: "rate limit exceeded".into(),
                diagnostics: None,
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
        service_context: conn.service_context.clone(),
    };

    let tx = conn.exec_tx.clone();
    // M-5: Per-task slot that `handle_command` / `async_mode` writes the enqueued
    // job ID into. After the command completes, we propagate it to the cumulative Vec.
    let per_task_job_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let job_ids_vec = conn.crawl_job_ids.clone();
    let perm_map = conn.permission_responders.clone();
    let task_slot = per_task_job_id.clone();
    tasks.spawn(async move {
        execute::handle_command(context, tx, task_slot, perm_map).await;
        // Propagate the job ID (if any) to the cumulative Vec for cancel-all support.
        if let Some(id) = per_task_job_id.lock().await.take() {
            job_ids_vec.lock().await.push(id);
        }
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

#[cfg(test)]
mod tests;

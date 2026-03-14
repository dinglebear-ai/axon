//! WebSocket connection state and message dispatch.
//!
//! Extracted from `web.rs` to stay under the 500-line module limit.
//! Contains `WsConnState`, the WS read/forward loops, and message routing
//! (execute, cancel, permission_response, read_file, acp_resume).

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
use crate::crates::services::acp::{PermissionResponderMap, SESSION_CACHE};
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
        conn_id,
        session_ownership: state.session_ownership.clone(),
        client_ip,
        rate_limiter: state.rate_limiter.clone(),
        stats_subscribed,
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

/// Detect `crawl_files` messages by their `"type"` field (L-2, P1-8) and track
/// `output_dir` / `job_id` for the crawl base dir and job dirs registries.
async fn track_crawl_files(
    msg: &str,
    base_dir: &Mutex<Option<PathBuf>>,
    job_dirs: &DashMap<String, PathBuf>,
) {
    // L-2: typed deserialization via MsgType — no string scan.
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

/// Dispatch a single parsed WS client message to the appropriate handler.
async fn handle_ws_message(conn: &WsConnState, client_msg: WsClientMsg, tasks: &mut JoinSet<()>) {
    match client_msg.msg_type.as_str() {
        "execute" => handle_execute_msg(conn, client_msg, tasks).await,
        "acp_resume" => handle_acp_resume(conn, &client_msg.session_id).await,
        "cancel" => handle_cancel_msg(conn, client_msg, tasks),
        "permission_response" => {
            route_permission_response(
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
        _ => {}
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
    let cancel_cfg = conn.conn_cfg.clone();
    let cancel_id = client_msg.id;

    tasks.spawn(async move {
        if !cancel_id.is_empty() {
            // Explicit job ID — cancel only that one.
            execute::handle_cancel(&cancel_mode, &cancel_id, tx, cancel_cfg).await;
            job_ids_arc.lock().await.retain(|id| id != &cancel_id);
        } else {
            // Implicit cancel — cancel ALL tracked job IDs.
            let ids: Vec<String> = job_ids_arc.lock().await.clone();
            for id in &ids {
                execute::handle_cancel(&cancel_mode, id, tx.clone(), cancel_cfg.clone()).await;
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

/// Build an `acp_resume_result` JSON string.
fn acp_resume_json(
    ok: bool,
    session_id: &str,
    reason: Option<&str>,
    replayed: Option<usize>,
) -> String {
    let mut v = serde_json::json!({"type": "acp_resume_result", "ok": ok});
    if !session_id.is_empty() {
        v["session_id"] = serde_json::json!(session_id);
    }
    if let Some(r) = reason {
        v["reason"] = serde_json::json!(r);
    }
    if let Some(n) = replayed {
        v["replayed"] = serde_json::json!(n);
    }
    v.to_string()
}

/// Handle `acp_resume` — reconnect to a cached ACP session and replay buffered
/// events.
///
/// M-6: Uses `read_replay_buffer()` which drains the buffer after the first
/// replay. The first reconnect receives all catch-up events; subsequent
/// reconnects see only events buffered after the previous replay.
///
/// Security (H-8): connection-binds session ownership on first resume.
async fn handle_acp_resume(conn: &WsConnState, session_id: &str) {
    let tx = &conn.exec_tx;

    if session_id.is_empty() {
        let _ = tx
            .send(acp_resume_json(false, "", Some("missing session_id"), None))
            .await;
        return;
    }

    let Some(cached) = SESSION_CACHE.get_by_session_id(session_id) else {
        let _ = tx
            .send(acp_resume_json(
                false,
                session_id,
                Some("session not found"),
                None,
            ))
            .await;
        log::info!("[ws] acp_resume: session_id={session_id} not found in cache");
        return;
    };

    // H-8: enforce connection-binding.
    let owner = conn
        .session_ownership
        .entry(session_id.to_string())
        .or_insert_with(|| conn.conn_id.clone())
        .value()
        .clone();
    if owner != conn.conn_id {
        log::warn!("[ws] acp_resume denied: session_id={session_id} bound to different connection");
        let _ = tx
            .send(acp_resume_json(
                false,
                "",
                Some("session bound to another connection"),
                None,
            ))
            .await;
        return;
    }

    // M-6: drain-on-read — first reconnect gets catch-up, buffer cleared after.
    let buffered = cached.read_replay_buffer();
    let replayed = buffered.len();
    for msg in buffered {
        let _ = tx.send(msg).await;
    }
    let _ = tx
        .send(acp_resume_json(true, session_id, None, Some(replayed)))
        .await;
    log::info!("[ws] acp_resume: session_id={session_id}, replayed {replayed} buffered event(s)");
}

/// Route a `permission_response` to the waiting ACP session.
/// Security (H-8): validates connection ownership for resumed sessions.
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

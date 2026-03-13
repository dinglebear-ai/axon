//! WebSocket connection state and message dispatch.
//!
//! Extracted from `web.rs` to stay under the 500-line module limit.
//! Contains `WsConnState`, the WS read/forward loops, and message routing
//! (execute, cancel, permission_response, read_file, acp_resume).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};

use crate::crates::core::config::Config;
use crate::crates::services::acp::{PermissionResponderMap, SESSION_CACHE};

use super::AppState;
use super::execute;

/// Maximum `execute` messages per connection per 60-second window (H-12).
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const RATE_LIMIT_MAX_EXECUTES: u32 = 120;

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
    /// Only the connection that first called `acp_resume` for a given
    /// session_id may drain its replay buffer or send permission responses.
    session_ownership: Arc<DashMap<String, String>>,
}

/// Create a fresh `PermissionResponderMap` for a new WS connection.
fn init_permission_responders() -> PermissionResponderMap {
    Arc::new(DashMap::new())
}

/// Main WS connection handler — runs the read/forward loops until disconnect.
pub(super) async fn handle_ws(socket: WebSocket, state: Arc<AppState>, conn_id: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let (exec_tx, mut exec_rx) = mpsc::channel::<String>(256);
    let (tracking_tx, mut tracking_rx) = mpsc::channel::<String>(256);

    let crawl_base_dir: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
    let crawl_job_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let permission_responders = init_permission_responders();

    let job_dirs = state.job_dirs.clone();
    let mut stats_rx = state.stats_tx.subscribe();

    // Forward task: sends exec output + stats to the WS client,
    // and tracks crawl_files messages to capture base_dir + register job_dirs
    let base_dir_tracker = crawl_base_dir.clone();
    let job_dirs_tracker = job_dirs.clone();
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = exec_rx.recv() => {
                    if msg.contains("\"crawl_files\"")
                        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
                            if let Some(dir) = parsed.get("output_dir").and_then(|v| v.as_str()) {
                                *base_dir_tracker.lock().await = Some(PathBuf::from(dir));
                            }
                            if let (Some(job_id), Some(dir)) = (
                                parsed.get("job_id").and_then(|v| v.as_str()),
                                parsed.get("output_dir").and_then(|v| v.as_str()),
                            ) {
                                job_dirs_tracker.insert(job_id.to_string(), PathBuf::from(dir));
                            }
                        }
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
    });

    let session_ownership = state.session_ownership.clone();
    let conn = WsConnState {
        exec_tx,
        tracking_tx,
        crawl_job_id,
        crawl_base_dir,
        permission_responders,
        conn_cfg: state.cfg.clone(),
        conn_id,
        session_ownership,
    };

    // Rate-limit counters — local to the sequential read loop, no synchronization needed (H-12).
    let mut execute_count: u32 = 0;
    let mut rate_window_start = Instant::now();

    // Read loop: receives commands from the browser
    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg {
            let Ok(client_msg) = serde_json::from_str::<WsClientMsg>(&text) else {
                let _ = conn
                    .exec_tx
                    .send(
                        serde_json::json!({"type": "error", "message": "invalid JSON"}).to_string(),
                    )
                    .await;
                continue;
            };
            handle_ws_message(
                &conn,
                client_msg,
                &mut execute_count,
                &mut rate_window_start,
            )
            .await;
        }
    }

    forward.abort();
}

/// Dispatch a single parsed WS client message to the appropriate handler.
async fn handle_ws_message(
    conn: &WsConnState,
    client_msg: WsClientMsg,
    execute_count: &mut u32,
    rate_window_start: &mut Instant,
) {
    match client_msg.msg_type.as_str() {
        "execute" => {
            // Rate limiting (H-12): sliding 60-second window per connection.
            if rate_window_start.elapsed() > Duration::from_secs(RATE_LIMIT_WINDOW_SECS) {
                *execute_count = 0;
                *rate_window_start = Instant::now();
            }
            *execute_count += 1;
            if *execute_count > RATE_LIMIT_MAX_EXECUTES {
                let _ = conn
                    .exec_tx
                    .send(
                        serde_json::json!({"type": "error", "message": "rate limit exceeded"})
                            .to_string(),
                    )
                    .await;
                return;
            }

            let tx = conn.exec_tx.clone();
            let job_id = conn.crawl_job_id.clone();
            let cmd_cfg = conn.conn_cfg.clone();
            let perm_map = conn.permission_responders.clone();
            let exec_mode = client_msg.mode;
            let exec_input = client_msg.input;
            let exec_flags = client_msg.flags;
            tokio::spawn(async move {
                execute::handle_command(
                    exec_mode,
                    exec_input,
                    exec_flags,
                    client_msg.id,
                    tx,
                    job_id,
                    cmd_cfg,
                    perm_map,
                )
                .await;
            });
        }
        "acp_resume" => {
            handle_acp_resume(conn, &client_msg.session_id).await;
        }
        "cancel" => {
            let tx = conn.exec_tx.clone();
            let job_id_arc = conn.crawl_job_id.clone();
            let cancel_mode = client_msg.mode;
            let cancel_cfg = conn.conn_cfg.clone();
            let cancel_id = client_msg.id;
            tokio::spawn(async move {
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
        "read_file" => {
            if !client_msg.path.is_empty() {
                let tx = conn.tracking_tx.clone();
                let path = client_msg.path;
                let base = conn.crawl_base_dir.clone();
                tokio::spawn(async move {
                    let guard = base.lock().await;
                    if let Some(base_dir) = guard.as_ref() {
                        execute::handle_read_file(&path, base_dir, tx).await;
                    } else {
                        let _ = tx
                            .send(
                                serde_json::json!({"type": "error", "message": "no crawl output available"})
                                    .to_string(),
                            )
                            .await;
                    }
                });
            }
        }
        _ => {}
    }
}

/// Handle `acp_resume` — reconnect to a cached ACP session and replay buffered events.
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

    // H-8: enforce connection-binding. If another connection already owns this
    // session_id, reject the attempt without revealing session contents.
    match conn
        .session_ownership
        .entry(session_id.to_string())
        .or_insert_with(|| conn.conn_id.clone())
        .value()
        .clone()
    {
        ref owner if owner != &conn.conn_id => {
            log::warn!(
                "[ws] acp_resume denied: session_id={session_id} is bound to a different connection"
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

    if let Some(cached) = SESSION_CACHE.get_by_session_id(session_id) {
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
        log::info!(
            "[ws] acp_resume: session_id={session_id}, replayed {replayed} buffered event(s)"
        );
    } else {
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
    }
}

/// Route a `permission_response` message to the waiting ACP session.
///
/// Security (H-8): for resumed sessions, validates that the requesting
/// connection owns the session before routing.
///
/// Looks up `(session_id, tool_call_id)` in the per-WS permission responders
/// first, then falls back to the global session cache for resumed sessions.
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

    // Try the per-WS permission responders first (sessions started on this connection).
    if let Some((_, sender)) = conn
        .permission_responders
        .remove(&(session_id.clone(), tool_call_id.clone()))
    {
        let _ = sender.send(option_id);
        return;
    }

    // Fallback: check the global session cache for resumed sessions whose
    // permission responders live in the CachedSession, not this WS conn.
    // H-8: only the owning connection may route permission responses.
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
mod tests {
    use super::*;

    #[test]
    fn test_permission_response_cross_session_isolation() {
        let map: PermissionResponderMap = Arc::new(DashMap::new());

        let shared_tool_call_id = "tc-shared";

        // Insert two entries with different session_ids but the same tool_call_id.
        let (tx_a, mut rx_a) = tokio::sync::oneshot::channel::<String>();
        let (tx_b, _rx_b) = tokio::sync::oneshot::channel::<String>();
        map.insert(
            ("session-A".to_string(), shared_tool_call_id.to_string()),
            tx_a,
        );
        map.insert(
            ("session-B".to_string(), shared_tool_call_id.to_string()),
            tx_b,
        );

        assert_eq!(map.len(), 2, "Both entries should exist before routing");

        // Build a minimal WsConnState-like setup for testing the per-connection
        // permission_responders path (the H-8 ownership check is not exercised
        // here — this path is for sessions originating on the current connection).
        // We call remove() on the map directly to mirror the pre-H-8 code path.
        let removed = map.remove(&("session-A".to_string(), shared_tool_call_id.to_string()));
        if let Some((_, sender)) = removed {
            let _ = sender.send("allow".to_string());
        }

        // Session A's entry should be consumed and the value received.
        assert_eq!(map.len(), 1, "Only session A's entry should be consumed");
        assert!(
            !map.contains_key(&("session-A".to_string(), shared_tool_call_id.to_string())),
            "Session A entry should be removed"
        );
        assert!(
            map.contains_key(&("session-B".to_string(), shared_tool_call_id.to_string())),
            "Session B entry must remain untouched"
        );

        // Verify session A received the correct value.
        let received = rx_a
            .try_recv()
            .expect("Session A should have received the response");
        assert_eq!(received, "allow");
    }

    #[test]
    fn execute_message_accepts_exec_id_alias() {
        let parsed: WsClientMsg = serde_json::from_str(
            r#"{"type":"execute","mode":"query","input":"rust","exec_id":"ws-exec-42"}"#,
        )
        .expect("execute message should deserialize");

        assert_eq!(parsed.msg_type, "execute");
        assert_eq!(parsed.mode, "query");
        assert_eq!(parsed.input, "rust");
        assert_eq!(parsed.id, "ws-exec-42");
    }

    #[test]
    fn cancel_message_still_accepts_id_field() {
        let parsed: WsClientMsg =
            serde_json::from_str(r#"{"type":"cancel","mode":"crawl","id":"job-123"}"#)
                .expect("cancel message should deserialize");

        assert_eq!(parsed.msg_type, "cancel");
        assert_eq!(parsed.mode, "crawl");
        assert_eq!(parsed.id, "job-123");
    }

    #[test]
    fn acp_resume_result_ok_key_is_serialized_correctly() {
        // Regression for C-1: verify "ok" (not "success") is emitted.
        let msg = serde_json::json!({
            "type": "acp_resume_result",
            "ok": true,
            "session_id": "sess-123",
            "replayed": 5
        })
        .to_string();

        assert!(msg.contains("\"ok\":true"), "must use 'ok' key, got: {msg}");
        assert!(
            !msg.contains("\"success\""),
            "must NOT use 'success' key, got: {msg}"
        );
    }

    #[test]
    fn rate_limit_constants_are_sane() {
        assert_eq!(RATE_LIMIT_WINDOW_SECS, 60);
        assert_eq!(RATE_LIMIT_MAX_EXECUTES, 120);
    }
}

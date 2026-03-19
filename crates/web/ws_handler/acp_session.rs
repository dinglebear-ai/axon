//! ACP session resume and permission response routing.
//!
//! Extracted from `ws_handler.rs` to stay under the 500-line module limit.

use serde_json::json;

use crate::crates::services::acp::SESSION_CACHE;

use super::WsConnState;

/// Build an `acp_resume_result` JSON string.
pub(super) fn acp_resume_json(
    ok: bool,
    session_id: &str,
    reason: Option<&str>,
    replayed: Option<usize>,
) -> String {
    let mut v = json!({"type": "acp_resume_result", "ok": ok});
    if !session_id.is_empty() {
        v["session_id"] = json!(session_id);
    }
    if let Some(r) = reason {
        v["reason"] = json!(r);
    }
    if let Some(n) = replayed {
        v["replayed"] = json!(n);
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
pub(super) async fn handle_acp_resume(conn: &WsConnState, session_id: &str) {
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
        tracing::info!(
            context = "ws",
            session_id,
            "acp_resume: session not found in cache"
        );
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
        tracing::warn!(
            context = "ws",
            session_id,
            "acp_resume denied: session bound to different connection"
        );
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
    tracing::info!(
        context = "ws",
        session_id,
        replayed,
        "acp_resume: replayed buffered events"
    );
}

/// Route a `permission_response` to the waiting ACP session.
/// Security (H-8): validates connection ownership for resumed sessions.
pub(super) fn route_permission_response(
    conn: &WsConnState,
    tool_call_id: String,
    option_id: String,
    session_id: String,
) {
    if tool_call_id.is_empty() || option_id.is_empty() {
        tracing::warn!(
            context = "ws",
            "permission_response with empty tool_call_id or option_id — ignoring"
        );
        return;
    }
    tracing::debug!(
        context = "ws",
        session_id,
        tool_call_id,
        "permission_response received"
    );

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
        tracing::warn!(
            context = "ws",
            session_id,
            tool_call_id,
            "permission_response denied: session not owned by this connection",
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

    tracing::warn!(
        context = "ws",
        session_id,
        tool_call_id,
        "permission_response for unknown key (already responded or wrong session)",
    );
}

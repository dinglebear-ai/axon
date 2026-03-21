//! ACP event dispatching and buffering for Pulse chat sessions.
//!
//! Extracted from `pulse_chat.rs` to stay under the 500-line module limit.

use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use crate::crates::services::acp::SESSION_CACHE;
use crate::crates::services::events::{LogLevel, ServiceEvent};
use crate::crates::services::types::AcpBridgeEvent;

use super::super::super::events::{
    CommandContext, acp_bridge_event_json, serialize_raw_output_event,
};
use super::super::service_calls::send_json_owned;

/// Send a single ACP `ServiceEvent` to the WS channel.
///
/// When the WS is disconnected (`tx.send()` fails), the serialized message is
/// buffered in the global session cache so it can be replayed on reconnect.
pub(in crate::crates::web) async fn dispatch_acp_event(
    event: ServiceEvent,
    tx: &mpsc::Sender<String>,
    ws_ctx: &CommandContext,
    agent_key: &str,
) {
    match event {
        ServiceEvent::Log { level, message } => {
            let truncated: String = message.chars().take(200).collect();
            match level {
                LogLevel::Info => tracing::info!(context = "pulse_chat", "{truncated}"),
                LogLevel::Warn => tracing::warn!(context = "pulse_chat", "{truncated}"),
                LogLevel::Error => tracing::error!(context = "pulse_chat", "{truncated}"),
            }
            send_json_owned(
                tx.clone(),
                ws_ctx.clone(),
                json!({"type": "status", "level": level, "message": message}),
            )
            .await;
        }
        ServiceEvent::AcpBridge { event } => {
            // Capture session_id from TurnResult for the session cache index.
            if let AcpBridgeEvent::TurnResult(ref result) = event {
                SESSION_CACHE.register_session_id(result.session_id.clone(), agent_key.to_string());
            }
            let raw_json = acp_bridge_event_json(&event);
            let event_type = raw_json
                .strip_prefix(r#"{"type":""#)
                .and_then(|rest| rest.find('"').map(|e| &rest[..e]));
            if !matches!(
                event_type,
                Some("assistant_delta") | Some("thinking_content") | Some("user_delta")
            ) {
                tracing::info!(
                    context = "pulse_chat",
                    event_type = event_type.unwrap_or("unknown"),
                    "ACP event",
                );
            }
            if let Some(envelope) = serialize_raw_output_event(ws_ctx, &raw_json) {
                send_or_buffer(tx, envelope, agent_key).await;
            }
        }
        ServiceEvent::EditorWrite { content, operation } => {
            tracing::info!(
                context = "pulse_chat",
                operation = ?operation,
                content_len = content.len(),
                "editor_update",
            );
            let standalone = json!({
                "type": "editor_update",
                "content": content,
                "operation": operation,
            })
            .to_string();
            send_or_buffer(tx, standalone, agent_key).await;
        }
        ServiceEvent::SynthesisDelta { text } => {
            send_json_owned(
                tx.clone(),
                ws_ctx.clone(),
                json!({"type": "synthesis_delta", "text": text}),
            )
            .await;
        }
    }
}

/// Try to send a WS message. On failure (WS disconnected), buffer it in the
/// global session cache so it can be replayed when the client reconnects.
async fn send_or_buffer(tx: &mpsc::Sender<String>, msg: String, agent_key: &str) {
    if tx.send(msg.clone()).await.is_err()
        && let Some(cached) = SESSION_CACHE.get_sync(agent_key)
    {
        cached.buffer_event(msg);
    }
}

/// Drive the ACP event loop for a persistent-connection turn.
///
/// Polls `result_rx` and `event_rx` concurrently; forwards each `ServiceEvent`
/// to the WS channel as it arrives. Returns after the result is received and
/// the event channel is drained.
pub(super) async fn drive_turn_events(
    mut result_rx: oneshot::Receiver<Result<(), String>>,
    mut event_rx: mpsc::Receiver<ServiceEvent>,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    agent_key: &str,
) -> Result<(), String> {
    loop {
        tokio::select! {
            biased;
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(event) => dispatch_acp_event(event, &tx, &ws_ctx, agent_key).await,
                    None => {
                        let run_result = result_rx
                            .try_recv()
                            .map_err(|_| "ACP turn result unavailable after channel close")?;
                        return run_result;
                    }
                }
            }
            result = &mut result_rx => {
                let run_result = result.map_err(|_| "ACP turn result channel dropped".to_string())?;
                while let Ok(event) = event_rx.try_recv() {
                    dispatch_acp_event(event, &tx, &ws_ctx, agent_key).await;
                }
                return run_result;
            }
        }
    }
}

/// Drive the ACP event loop for a non-persistent path (pulse_chat_probe).
pub(super) async fn run_acp_event_loop(
    mut task: tokio::task::JoinHandle<Result<(), String>>,
    mut event_rx: mpsc::Receiver<ServiceEvent>,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    task_name: &'static str,
) -> Result<Option<String>, String> {
    loop {
        tokio::select! {
            biased;
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(event) => dispatch_acp_event(event, &tx, &ws_ctx, "").await,
                    None => {
                        let run_result = (&mut task)
                            .await
                            .map_err(|e| format!("failed to join {task_name} task: {e}"))?;
                        run_result?;
                        break;
                    }
                }
            }
            join_result = &mut task => {
                let run_result = join_result
                    .map_err(|e| format!("failed to join {task_name} task: {e}"))?;
                run_result?;
                while let Ok(event) = event_rx.try_recv() {
                    dispatch_acp_event(event, &tx, &ws_ctx, "").await;
                }
                break;
            }
        }
    }
    Ok(None)
}

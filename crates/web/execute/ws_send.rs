use super::context::ExecCommandContext;
use super::events::{CommandDonePayload, CommandErrorPayload, WsEventV2, serialize_v2_event};
use tokio::sync::mpsc;

/// Emit a serialized JSON string on `tx` using best-effort `try_send`.
///
/// Uses `try_send` instead of `send().await` so that a full channel is detected
/// immediately as backpressure rather than blocking the caller until capacity
/// frees up.  When the channel is full we emit a visible truncation sentinel so
/// the browser knows output was lost.  The sentinel itself is best-effort — if
/// the channel is still full we accept the loss rather than blocking.
///
/// Appropriate for non-terminal events (start, output lines) where dropping
/// under backpressure is acceptable. Terminal events (done, error) must use
/// [`send_reliable`] to guarantee delivery.
fn send_or_sentinel(tx: &mpsc::Sender<String>, msg: String) {
    match tx.try_send(msg.clone()) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            // Preserve the original event type in the truncation sentinel so
            // the client can attribute the loss to the correct stream.
            let event_type = serde_json::from_str::<serde_json::Value>(&msg)
                .ok()
                .and_then(|v| v["type"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "log".to_string());
            let sentinel = serde_json::json!({
                "type": event_type,
                "line": "[output truncated — WebSocket channel full]",
                "truncated": true
            })
            .to_string();
            let _ = tx.try_send(sentinel); // best-effort only
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

/// Reliably send a terminal event via async `.send().await`.
///
/// Terminal events (`command.done`, `command.error`) must not be dropped under
/// backpressure — a lost terminal event causes the client to hang indefinitely,
/// never clearing `isProcessing` state.  This function blocks until channel
/// capacity is available (or the receiver is dropped).
async fn send_reliable(tx: &mpsc::Sender<String>, msg: String) {
    let _ = tx.send(msg).await;
}

pub(super) fn send_command_start(tx: &mpsc::Sender<String>, context: &ExecCommandContext) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandStart {
        ctx: context.to_ws_ctx(),
    }) {
        send_or_sentinel(tx, v2);
    }
}

pub(super) fn send_command_output_line(
    tx: &mpsc::Sender<String>,
    context: &super::events::CommandContext,
    line: String,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandOutputLine {
        ctx: context.clone(),
        line,
    }) {
        send_or_sentinel(tx, v2);
    }
}

pub(super) async fn send_done_dual(
    tx: &mpsc::Sender<String>,
    context: &super::events::CommandContext,
    exit_code: i32,
    elapsed_ms: Option<u64>,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandDone {
        ctx: context.clone(),
        payload: CommandDonePayload {
            exit_code,
            elapsed_ms,
        },
    }) {
        send_reliable(tx, v2).await;
    }
}

pub(super) async fn send_error_dual(
    tx: &mpsc::Sender<String>,
    context: &super::events::CommandContext,
    message: String,
    elapsed_ms: Option<u64>,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandError {
        ctx: context.clone(),
        payload: CommandErrorPayload {
            message,
            elapsed_ms,
        },
    }) {
        send_reliable(tx, v2).await;
    }
}

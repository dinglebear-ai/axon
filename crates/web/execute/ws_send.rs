use super::context::ExecCommandContext;
use super::events::{CommandDonePayload, CommandErrorPayload, WsEventV2, serialize_v2_event};
use tokio::sync::mpsc;

/// Emit a serialized JSON string on `tx`.
///
/// Uses `try_send` instead of `send().await` so that a full channel is detected
/// immediately as backpressure rather than blocking the caller until capacity
/// frees up.  When the channel is full we emit a visible truncation sentinel so
/// the browser knows output was lost.  The sentinel itself is best-effort — if
/// the channel is still full we accept the loss rather than blocking.
fn send_or_sentinel(tx: &mpsc::Sender<String>, msg: String) {
    match tx.try_send(msg) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            let sentinel = serde_json::json!({
                "type": "log",
                "line": "[output truncated — WebSocket channel full]"
            })
            .to_string();
            let _ = tx.try_send(sentinel); // best-effort only
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

pub(super) async fn send_command_start(tx: &mpsc::Sender<String>, context: &ExecCommandContext) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandStart {
        ctx: context.to_ws_ctx(),
    }) {
        send_or_sentinel(tx, v2);
    }
}

pub(super) async fn send_command_output_line(
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
        send_or_sentinel(tx, v2);
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
        send_or_sentinel(tx, v2);
    }
}

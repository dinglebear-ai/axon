use super::context::ExecCommandContext;
use super::events::{CommandDonePayload, CommandErrorPayload, WsEventV2, serialize_v2_event};
use tokio::sync::mpsc;

/// Emit a serialized JSON string on `tx`.
///
/// When the channel is at capacity (slow client + high-volume crawl), the send
/// will fail immediately because all WS-send channels are bounded (`channel(256)`).
/// Rather than silently discarding the message we emit a visible truncation
/// sentinel so the browser knows output was lost.  The sentinel send itself is
/// best-effort — if the channel is still full we accept the loss rather than
/// blocking indefinitely.
async fn send_or_sentinel(tx: &mpsc::Sender<String>, msg: String) {
    if tx.send(msg).await.is_err() {
        let sentinel = serde_json::json!({
            "type": "log",
            "line": "[output truncated — WebSocket channel full]"
        })
        .to_string();
        let _ = tx.send(sentinel).await; // best-effort only
    }
}

pub(super) async fn send_command_start(tx: &mpsc::Sender<String>, context: &ExecCommandContext) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandStart {
        ctx: context.to_ws_ctx(),
    }) {
        send_or_sentinel(tx, v2).await;
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
        send_or_sentinel(tx, v2).await;
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
        send_or_sentinel(tx, v2).await;
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
        send_or_sentinel(tx, v2).await;
    }
}

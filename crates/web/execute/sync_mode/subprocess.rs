use std::time::Instant;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;
use tokio::sync::mpsc;

use super::super::context::ExecCommandContext;
use super::super::exe::strip_ansi;
use super::super::files;
use super::super::ws_send::{send_command_output_line, send_done_dual, send_error_dual};
use super::service_calls::send_json_owned;

use super::super::events::CommandContext;

/// Read stdout from the child process, stream non-screenshot JSON to the WS
/// channel, and accumulate screenshot JSON entries for post-exit artifact
/// forwarding. Returns the accumulated screenshot JSON objects.
///
/// In screenshot mode, JSON lines are accumulated only — NOT streamed inline.
/// This prevents double emission: `send_screenshot_files_from_json` produces
/// the canonical `artifact.list` payload after the process exits.
async fn read_stdout(
    stdout: Option<ChildStdout>,
    is_screenshot: bool,
    tx: mpsc::Sender<String>,
    ctx: CommandContext,
) -> Vec<serde_json::Value> {
    let Some(stdout) = stdout else {
        return Vec::new();
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut screenshot_jsons: Vec<serde_json::Value> = Vec::new();
    let mut stdout_accum = String::new();
    let mut saw_json_line = false;

    while let Ok(Some(line)) = lines.next_line().await {
        let clean = strip_ansi(&line);
        if clean.trim().is_empty() {
            continue;
        }
        // FINDING-12: only accumulate before we've seen the first JSON line —
        // after that, stdout_accum is only used in the fallback path below which
        // is skipped when saw_json_line is true, so continued accumulation wastes
        // memory for long streaming outputs.
        if !saw_json_line {
            if !stdout_accum.is_empty() {
                stdout_accum.push('\n');
            }
            stdout_accum.push_str(&clean);
        }
        match serde_json::from_str::<serde_json::Value>(&clean) {
            Ok(parsed) if parsed.is_object() || parsed.is_array() => {
                saw_json_line = true;
                if is_screenshot {
                    screenshot_jsons.push(parsed);
                } else {
                    send_json_owned(tx.clone(), ctx.clone(), parsed).await;
                }
            }
            Ok(_) | Err(_) => {
                send_command_output_line(&tx, &ctx, clean);
            }
        }
    }

    // If no structured JSON was seen, try parsing the entire accumulated output
    // as a single JSON object (some commands emit one blob, not streaming lines).
    if !saw_json_line
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stdout_accum.trim())
    {
        if is_screenshot {
            screenshot_jsons.push(parsed);
        } else {
            send_json_owned(tx, ctx, parsed).await;
        }
    }

    screenshot_jsons
}

/// Read stderr from the child process, deduplicating consecutive identical lines,
/// and forward each unique line as a `log` event to the WS channel.
async fn read_stderr(stderr: Option<tokio::process::ChildStderr>, tx: mpsc::Sender<String>) {
    let Some(stderr) = stderr else { return };
    let mut lines = BufReader::new(stderr).lines();
    let mut last_stderr = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let clean = strip_ansi(&line);
        if clean.trim().is_empty() {
            continue;
        }
        if clean == last_stderr {
            continue;
        }
        last_stderr.clone_from(&clean);
        if tx
            .send(json!({"type": "log", "line": clean}).to_string())
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Finalise after both stream readers have completed: wait for the child exit
/// status and send the appropriate `done` or `error` event.
///
/// Screenshot artifacts accumulated during stdout reading are forwarded before
/// the done/error event so the frontend receives them even on non-zero exit.
async fn finalize_exit(
    mut child: tokio::process::Child,
    screenshot_jsons: Vec<serde_json::Value>,
    is_screenshot: bool,
    tx: &mpsc::Sender<String>,
    ws_ctx: &CommandContext,
    elapsed: u64,
) {
    let status = child.wait().await;

    // Always forward accumulated screenshot artifacts before done/error —
    // partial results are better than nothing if the process exits non-zero.
    if is_screenshot && !screenshot_jsons.is_empty() {
        files::send_screenshot_files_from_json(&screenshot_jsons, tx, ws_ctx).await;
    }

    match status {
        Ok(exit) => {
            let code = exit.code().unwrap_or(-1);
            if code == 0 {
                send_done_dual(tx, ws_ctx, code, Some(elapsed)).await;
            } else {
                send_error_dual(tx, ws_ctx, format!("exit code {code}"), Some(elapsed)).await;
            }
        }
        Err(e) => {
            send_error_dual(tx, ws_ctx, format!("wait failed: {e}"), None).await;
        }
    }
}

/// Subprocess-backed sync handler — used for modes not yet wired to direct dispatch.
///
/// Reads stdout/stderr from the child process and streams events to the WS
/// sender. Screenshot JSON payloads are accumulated and forwarded as
/// artifact entries after the process exits.
pub(crate) async fn handle_sync_command(
    mut child: tokio::process::Child,
    context: &ExecCommandContext,
    tx: &mpsc::Sender<String>,
    start: Instant,
) {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_tx = tx.clone();
    let stderr_tx = tx.clone();
    let is_screenshot = context.mode == "screenshot";
    let ws_ctx = context.to_ws_ctx();
    let stdout_ctx = ws_ctx.clone();

    let stdout_task = tokio::spawn(read_stdout(stdout, is_screenshot, stdout_tx, stdout_ctx));
    let stderr_task = tokio::spawn(read_stderr(stderr, stderr_tx));

    let (stdout_result, _) = tokio::join!(stdout_task, stderr_task);
    let screenshot_jsons = stdout_result.unwrap_or_default();
    let elapsed = start.elapsed().as_millis() as u64;

    finalize_exit(child, screenshot_jsons, is_screenshot, tx, &ws_ctx, elapsed).await;
}

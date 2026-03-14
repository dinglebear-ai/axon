//! Command execution bridge for `axon serve`.
//! Validates frontend requests, calls services directly for sync modes, enqueues
//! async jobs with fire-and-forget semantics, and streams output over WebSocket.
//!
//! # Execution paths
//! - **Async direct modes** (`crawl`, `extract`, `embed`, `github`, `reddit`, `youtube`):
//!   `async_mode::handle_async_command` — direct service enqueue, fire-and-forget.
//! - **Sync direct modes** (scrape, map, query, retrieve, ask, search, research,
//!   stats, sources, domains, doctor, status, pulse_chat):
//!   `sync_mode::handle_sync_direct` — direct service call, awaited inline.
//! - **Sync subprocess fallback** (suggest, screenshot, evaluate, sessions,
//!   dedupe, debug, refresh): spawns the `axon` binary until direct dispatch is wired.
mod args;
mod async_mode;
mod cancel;
pub(crate) mod constants;
mod context;
pub(crate) mod events;
mod exe;
pub(crate) mod files;
pub(crate) mod mcp_config;
pub mod overrides;
pub(crate) mod session_guard;
mod sync_mode;
mod ws_send;

#[cfg(test)]
#[path = "execute/tests/ws_event_v2_tests.rs"]
mod ws_event_v2_tests;

#[cfg(test)]
#[path = "execute/tests/acp_ws_event_tests.rs"]
mod acp_ws_event_tests;

#[cfg(test)]
#[path = "execute/tests/ws_protocol_tests.rs"]
mod ws_protocol_tests;

pub(crate) use context::ExecCommandContext;
pub(crate) use files::handle_read_file;

#[cfg(test)]
fn build_args(mode: &str, input: &str, flags: &serde_json::Value) -> Vec<String> {
    args::build_args(mode, input, flags)
}

#[cfg(test)]
fn strip_ansi(s: &str) -> String {
    exe::strip_ansi(s)
}

#[cfg(test)]
fn allowed_modes() -> &'static [&'static str] {
    ALLOWED_MODES
}

#[cfg(test)]
fn allowed_flags() -> &'static [(&'static str, &'static str)] {
    ALLOWED_FLAGS
}

#[cfg(test)]
fn direct_sync_modes() -> &'static [&'static str] {
    sync_mode::DIRECT_SYNC_MODES
}

#[cfg(test)]
fn async_modes() -> &'static [&'static str] {
    ASYNC_MODES
}

// Public re-exports for integration tests in tests/web_ws_async_fire_and_forget.rs.
// These forward to the same internal constants/functions but are exposed via the
// public `execute` module path so integration tests can import them without
// reaching into private submodule internals.
pub fn async_modes_pub() -> &'static [&'static str] {
    ASYNC_MODES
}

pub fn direct_sync_modes_pub() -> &'static [&'static str] {
    sync_mode::DIRECT_SYNC_MODES
}

pub fn allowed_modes_pub() -> &'static [&'static str] {
    ALLOWED_MODES
}

pub fn is_valid_cancel_job_id_pub(job_id: &str) -> bool {
    cancel::is_valid_cancel_job_id(job_id)
}

use crate::crates::core::config::Config;
use constants::{ACP_MODES, ALLOWED_FLAGS, ALLOWED_MODES, ASYNC_MODES};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};

fn resolve_exe() -> Result<std::path::PathBuf, String> {
    exe::resolve_exe()
}

#[cfg(test)]
fn cancel_ok_from_output(parsed: Option<&serde_json::Value>, status_success: bool) -> bool {
    cancel::cancel_ok_from_output(parsed, status_success)
}

#[cfg(test)]
fn is_valid_cancel_job_id(job_id: &str) -> bool {
    cancel::is_valid_cancel_job_id(job_id)
}

#[cfg(test)]
fn send_command_output_line(
    tx: &mpsc::Sender<String>,
    context: &events::CommandContext,
    line: String,
) {
    ws_send::send_command_output_line(tx, context, line)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) async fn send_done_dual(
    tx: &mpsc::Sender<String>,
    context: &events::CommandContext,
    exit_code: i32,
    elapsed_ms: Option<u64>,
) {
    ws_send::send_done_dual(tx, context, exit_code, elapsed_ms).await
}

pub(super) async fn send_error_dual(
    tx: &mpsc::Sender<String>,
    context: &events::CommandContext,
    message: String,
    elapsed_ms: Option<u64>,
) {
    ws_send::send_error_dual(tx, context, message, elapsed_ms).await
}

async fn handle_sync_command(
    child: tokio::process::Child,
    context: &ExecCommandContext,
    tx: &mpsc::Sender<String>,
    start: Instant,
) {
    sync_mode::handle_sync_command(child, context, tx, start).await
}

pub(super) async fn handle_cancel(
    mode: &str,
    job_id: &str,
    tx: mpsc::Sender<String>,
    cfg: Arc<Config>,
) {
    cancel::handle_cancel(mode, job_id, tx, cfg).await
}

pub(crate) async fn handle_command(
    context: ExecCommandContext,
    tx: mpsc::Sender<String>,
    crawl_job_id: Arc<Mutex<Option<String>>>,
    permission_responders: crate::crates::services::acp::PermissionResponderMap,
) {
    let ws_ctx = context.to_ws_ctx();
    let mode = context.mode.clone();
    let input = context.input.clone();
    let flags = context.flags.clone();

    if !ALLOWED_MODES.contains(&mode.as_str()) {
        send_error_dual(&tx, &ws_ctx, format!("unknown mode: {mode}"), None).await;
        return;
    }

    // Reject unknown flag keys before processing; only whitelisted keys are accepted.
    if let Some(obj) = flags.as_object() {
        let allowed_keys: std::collections::HashSet<&str> =
            ALLOWED_FLAGS.iter().map(|(k, _)| *k).collect();
        let unknown: Vec<&String> = obj
            .keys()
            .filter(|k| !allowed_keys.contains(k.as_str()))
            .collect();
        if !unknown.is_empty() {
            let list = unknown
                .into_iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            send_error_dual(&tx, &ws_ctx, format!("unknown flag key(s): {list}"), None).await;
            return;
        }
    }

    // Async direct modes (crawl, extract, embed, github, reddit, youtube) — fire-and-forget direct service dispatch:
    // enqueue the job and return immediately with the job ID.
    // No subprocess is spawned; no polling loop is run.
    if ASYNC_MODES.contains(&mode.as_str()) {
        ws_send::send_command_start(&tx, &context);
        async_mode::handle_async_command(context, tx, crawl_job_id).await;
        return;
    }

    // Sync direct modes (scrape, map, query, retrieve, ask, search, research,
    // stats, sources, domains, doctor, status, pulse_chat) — call services directly.
    if let Some(params) = sync_mode::classify_sync_direct(&context) {
        // SEC-8 / PERF-1 / PERF-10: acquire ACP session permit for pulse_chat and
        // pulse_chat_probe to bound concurrent spawn_blocking threads.
        let permit_result = acquire_acp_permit(&mode, &tx, &ws_ctx).await;
        let _acp_permit = match permit_result {
            Ok(p) => p,
            Err(()) => return, // error already sent to client
        };
        ws_send::send_command_start(&tx, &context);
        sync_mode::handle_sync_direct(params, tx, ws_ctx, permission_responders).await;
        drop(_acp_permit); // explicit drop for clarity — permit held for full command duration

        return;
    }

    ws_send::send_command_start(&tx, &context);
    dispatch_subprocess_fallback(context, &mode, &input, &flags, tx, ws_ctx).await;
}

/// Acquire a semaphore permit for ACP sessions (`pulse_chat` / `pulse_chat_probe`).
///
/// Returns `Ok(Some(permit))` when a permit is available, `Ok(None)` for non-ACP
/// modes, and `Err(())` when the semaphore is exhausted — in that case an error
/// event has already been sent to the client.
///
/// Waits up to 30 seconds for a slot to open before failing, so bursts of
/// concurrent ACP requests queue briefly instead of being rejected immediately.
async fn acquire_acp_permit(
    mode: &str,
    tx: &mpsc::Sender<String>,
    ws_ctx: &events::CommandContext,
) -> Result<Option<tokio::sync::SemaphorePermit<'static>>, ()> {
    if ACP_MODES.contains(&mode) {
        // M-11: Notify client before potentially blocking on the semaphore,
        // so a 30-second hang has visible feedback in the browser.
        // Uses `command.output.line` — a WsEventV2 type the TypeScript client
        // actually parses — instead of the raw `status` type which has no
        // handler in the WS message dispatcher.
        if crate::crates::web::ACP_SESSION_SEMAPHORE.available_permits() == 0 {
            ws_send::send_command_output_line(
                tx,
                ws_ctx,
                "Waiting for available session slot...".to_string(),
            );
        }
        const ACP_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
        match tokio::time::timeout(
            ACP_ACQUIRE_TIMEOUT,
            crate::crates::web::ACP_SESSION_SEMAPHORE.acquire(),
        )
        .await
        {
            Ok(Ok(permit)) => Ok(Some(permit)),
            Ok(Err(_)) => {
                // Semaphore closed — should never happen with a static semaphore.
                send_error_dual(
                    tx,
                    ws_ctx,
                    "ACP session semaphore closed unexpectedly".to_string(),
                    None,
                )
                .await;
                Err(())
            }
            Err(_) => {
                send_error_dual(
                    tx,
                    ws_ctx,
                    "ACP session queue full — timed out after 30s waiting for a slot".to_string(),
                    None,
                )
                .await;
                Err(())
            }
        }
    } else {
        Ok(None)
    }
}

/// Subprocess fallback for modes not yet wired to direct dispatch.
///
/// Covers suggest, screenshot, evaluate, sessions, dedupe, debug, refresh.
/// TODO: direct dispatch for remaining modes once `!Send` constraints are resolved.
async fn dispatch_subprocess_fallback(
    context: ExecCommandContext,
    mode: &str,
    input: &str,
    flags: &serde_json::Value,
    tx: mpsc::Sender<String>,
    ws_ctx: events::CommandContext,
) {
    // P2-3: resolve_exe() calls std::path::Path::exists() (blocking I/O) on
    // multiple candidate paths.  Move it off the async runtime thread.
    let exe = match tokio::task::spawn_blocking(resolve_exe).await {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            log::error!("[execute] resolve_exe failed: {e}");
            send_error_dual(&tx, &ws_ctx, "cannot find axon binary".to_string(), None).await;
            return;
        }
        Err(join_err) => {
            log::error!("[execute] resolve_exe join error: {join_err}");
            send_error_dual(&tx, &ws_ctx, "resolve_exe join error".to_string(), None).await;
            return;
        }
    };
    let args = args::build_args(mode, input, flags);
    let start = Instant::now();
    let child = Command::new(&exe)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let child = match child {
        Ok(c) => c,
        Err(e) => {
            send_error_dual(
                &tx,
                &ws_ctx,
                format!("spawn failed: {e} (exe: {})", exe.display()),
                None,
            )
            .await;
            return;
        }
    };
    handle_sync_command(child, &context, &tx, start).await;
}

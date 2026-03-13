mod acp_adapter;
mod dispatch;
mod params;
mod pulse_chat;
mod service_calls;
mod subprocess;
mod types;

use std::sync::LazyLock;
use std::time::Instant;

use tokio::sync::{Semaphore, mpsc};

use crate::crates::services::acp as acp_svc;

use super::context::ExecCommandContext;
use super::events::CommandContext;

use dispatch::dispatch_service;
use params::extract_params;
use service_calls::{send_done_owned, send_error_owned};
use types::ServiceMode;

pub(super) use subprocess::handle_sync_command;
pub(super) use types::DIRECT_SYNC_MODES;
pub(super) use types::DirectParams;

/// Maximum concurrent non-ACP sync mode executions.
///
/// ACP modes (`pulse_chat`, `pulse_chat_probe`) are excluded from this gate —
/// they are already bounded by `crate::crates::web::ACP_SESSION_SEMAPHORE`
/// acquired in `execute.rs` before `handle_sync_direct` is called.  Adding
/// them here would be a dual-acquisition that cuts effective ACP capacity and
/// creates two inconsistent sources of truth for the session limit.
///
/// Override at runtime via the `AXON_MAX_SYNC_CONCURRENT` environment variable.
static SYNC_MODE_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    let limit = std::env::var("AXON_MAX_SYNC_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&limit| limit > 0)
        .unwrap_or(16);
    Semaphore::new(limit)
});

/// Returns `true` for modes that already hold their own concurrency permit
/// before entering `handle_sync_direct`, so they must be exempted from
/// `SYNC_MODE_SEMAPHORE` to avoid dual-acquisition.
///
/// Mirrors `constants::ACP_MODES` (used by `execute.rs::acquire_acp_permit`)
/// at the enum level.  If ACP modes change, update both sites and the constant.
fn is_acp_mode(mode: &ServiceMode) -> bool {
    // NOTE: keep in sync with super::constants::ACP_MODES
    matches!(mode, ServiceMode::PulseChat | ServiceMode::PulseChatProbe)
}

/// Classify a mode string and extract all request parameters into owned values.
///
/// This is the **only** place in the direct-dispatch path where a `String` is
/// borrowed as `&str` — the call to `ServiceMode::from_str` and the
/// `extract_params` helpers.  By doing all classification here, in a plain
/// (non-async) function, no `&str` or `&ExecCommandContext` borrow ever enters
/// an async state machine, which is the root cause of Rust's HRTB `Send` false
/// positives when spawning with `tokio::task::spawn`.
///
/// Returns `None` when the mode is not a recognised `DIRECT_SYNC_MODES` entry.
pub(super) fn classify_sync_direct(context: &ExecCommandContext) -> Option<DirectParams> {
    // L-7: Accept full context directly — no synthetic rebuild needed.
    ServiceMode::from_str(&context.mode)?;
    extract_params(context, &context.flags)
}

/// Execute a pre-classified direct-dispatch request.
///
/// All parameters are fully owned — no `String → &str` conversion happens
/// inside this `async fn`, so the generated `Future` satisfies `Send + 'static`
/// and can be submitted to `tokio::task::spawn` without triggering Rust's HRTB
/// `Send` false positive.
///
/// For non-ACP modes a permit from `SYNC_MODE_SEMAPHORE` is acquired before
/// dispatching.  This bounds the number of concurrent `evaluate` (and similar)
/// executions, each of which spawns an OS thread and a dedicated Tokio runtime.
/// The permit is held for the full duration of the service call and released on
/// drop when the function returns.
pub(super) async fn handle_sync_direct(
    params: DirectParams,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    permission_responders: acp_svc::PermissionResponderMap,
) {
    let start = Instant::now();

    // Acquire a concurrency permit for all non-ACP modes.  ACP modes already
    // hold ACP_SESSION_SEMAPHORE at this point (acquired in execute.rs).
    let _permit = if !is_acp_mode(&params.mode) {
        match SYNC_MODE_SEMAPHORE.acquire().await {
            Ok(permit) => Some(permit),
            Err(_) => {
                // Semaphore was closed — this should never happen in normal
                // operation (the static is never explicitly closed).
                send_error_owned(
                    tx,
                    ws_ctx,
                    "sync semaphore closed unexpectedly".to_string(),
                    Some(start.elapsed().as_millis() as u64),
                )
                .await;
                return;
            }
        }
    } else {
        None
    };

    // dispatch_service takes full ownership — no borrows cross any .await.
    let svc_result =
        dispatch_service(params, tx.clone(), ws_ctx.clone(), permission_responders).await;
    let elapsed_ms = Some(start.elapsed().as_millis() as u64);

    match svc_result {
        Ok(()) => send_done_owned(tx, ws_ctx, 0, elapsed_ms).await,
        Err(e) => send_error_owned(tx, ws_ctx, e.to_string(), elapsed_ms).await,
    }

    // _permit drops here, releasing the semaphore slot.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_sync_modes_not_in_async_modes() {
        use crate::crates::web::execute::constants::ASYNC_MODES;
        for mode in DIRECT_SYNC_MODES {
            assert!(
                !ASYNC_MODES.contains(mode),
                "mode '{mode}' must not be in both DIRECT_SYNC_MODES and ASYNC_MODES"
            );
        }
    }

    #[test]
    fn direct_sync_modes_all_in_allowed_modes() {
        use crate::crates::web::execute::constants::ALLOWED_MODES;
        for mode in DIRECT_SYNC_MODES {
            assert!(
                ALLOWED_MODES.contains(mode),
                "mode '{mode}' in DIRECT_SYNC_MODES must also be in ALLOWED_MODES"
            );
        }
    }
}

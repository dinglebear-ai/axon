//! ACP runtime: `run_prompt_turn` and `run_session_probe` orchestration.
//!
//! Contains the core logic for establishing an ACP session (spawn adapter,
//! initialize, session setup) and running prompt turns or probes.
//!
//! Session-level sub-functions (spawn, connect, setup) live in `session.rs`.

use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::{
    AcpAdapterCommand, AcpPromptTurnRequest, AcpSessionProbeRequest,
};
use agent_client_protocol::{
    Agent, ClientSideConnection, ContentBlock, InitializeRequest, PromptRequest, SessionId,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::adapters::{
    append_codex_model_override, append_gemini_model_override, is_codex_adapter, is_gemini_adapter,
};
use super::bridge::{AcpRuntimeState, finalize_successful_turn};
use super::session::{
    apply_config_and_model, initialize_connection, setup_session, spawn_adapter_with_io,
};
use super::{AcpSessionSetupRequest, PermissionResponderMap};

// ── AdapterGuard (FIX A-8/PERF-3) ──────────────────────────────────────────

/// RAII guard that kills the adapter subprocess on drop, covering all error
/// paths without explicit kill calls.
pub(super) struct AdapterGuard(pub(super) Option<tokio::process::Child>);

impl AdapterGuard {
    pub(super) fn new(child: tokio::process::Child) -> Self {
        Self(Some(child))
    }

    /// Take ownership of the child process (disarms the guard).
    pub(super) fn take(&mut self) -> Option<tokio::process::Child> {
        self.0.take()
    }
}

impl Drop for AdapterGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.0 {
            let _ = child.start_kill();
        }
    }
}

// ── EstablishedSession ───────────────────────────────────────────────────────

/// Result of establishing an ACP session: connection + session ID + exit watcher.
pub struct EstablishedSession {
    pub(super) conn: ClientSideConnection,
    pub(super) session_id: SessionId,
    pub(super) exit_rx: tokio::sync::oneshot::Receiver<String>,
    pub(super) runtime_state: Arc<AcpRuntimeState>,
}

// ── establish_acp_session ────────────────────────────────────────────────────

/// Orchestrate: spawn adapter → initialize → session setup → config/model apply.
///
/// FIX H-1: Single entry point for both prompt-turn and probe flows.
/// Sub-functions are in `session.rs` to respect the 500-line file limit.
pub(super) async fn establish_acp_session(
    adapter: AcpAdapterCommand,
    initialize: InitializeRequest,
    session_setup: AcpSessionSetupRequest,
    model: Option<&str>,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: &PermissionResponderMap,
) -> Result<EstablishedSession, String> {
    let adapter = append_codex_model_override(adapter, model)
        .map_err(|err| format!("invalid model override: {err}"))?;
    let adapter = append_gemini_model_override(adapter, model)
        .map_err(|err| format!("invalid model override: {err}"))?;
    let codex_adapter = is_codex_adapter(&adapter);
    let gemini_adapter = is_gemini_adapter(&adapter);

    let msg = "ACP runtime: spawning adapter process".to_string();
    crate::crates::core::logging::log_info(&msg);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    let spawned = spawn_adapter_with_io(adapter.clone(), tx)?;
    let (conn, runtime_state, exit_rx) =
        initialize_connection(spawned, &adapter, initialize, tx, permission_responders).await?;
    let (session_id, initial_config_options) = setup_session(&conn, session_setup, tx).await?;
    let latest_config_options = apply_config_and_model(
        &conn,
        &session_id,
        initial_config_options,
        model,
        codex_adapter,
        gemini_adapter,
        tx,
    )
    .await?;
    *runtime_state.config_options.borrow_mut() = latest_config_options;

    Ok(EstablishedSession {
        conn,
        session_id,
        exit_rx,
        runtime_state,
    })
}

// ── wait_for_adapter_exit ────────────────────────────────────────────────────

/// Close the ACP connection and wait up to 10 s for the adapter to flush its
/// session file and exit cleanly.
///
/// Called after a successful prompt turn to avoid SIGKILLing the adapter before
/// it writes the `.jsonl` session file.  See the `kill_on_drop` note in
/// `run_prompt_turn`.
async fn wait_for_adapter_exit(
    conn: ClientSideConnection,
    runtime_state: Arc<AcpRuntimeState>,
    exit_rx: tokio::sync::oneshot::Receiver<String>,
    session_id_str: &str,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) {
    // Drop connection handles → EOF on adapter stdin → adapter flushes + exits.
    drop(conn);
    drop(runtime_state);

    let msg = format!(
        "ACP runtime: connection closed; waiting up to 10 s for adapter to exit \
         and write session file (session_id={session_id_str})"
    );
    crate::crates::core::logging::log_info(&msg);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    match tokio::time::timeout(std::time::Duration::from_secs(10), exit_rx).await {
        Ok(Ok(crash_msg)) => {
            let msg = format!("ACP adapter exited with error after turn: {crash_msg}");
            crate::crates::core::logging::log_warn(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: msg,
                },
            )
            .await;
        }
        Ok(Err(_)) => {
            // oneshot sender dropped = process exited with status 0
            let msg = format!(
                "ACP adapter exited cleanly; session file should be on disk \
                 (session_id={session_id_str})"
            );
            crate::crates::core::logging::log_info(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: msg,
                },
            )
            .await;
        }
        Err(_) => {
            let msg = format!(
                "ACP adapter did not exit within 10 s after connection close \
                 (session_id={session_id_str}); forcing kill via kill_on_drop"
            );
            crate::crates::core::logging::log_warn(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: msg,
                },
            )
            .await;
        }
    }
}

// ── run_prompt_turn ──────────────────────────────────────────────────────────

pub(super) async fn run_prompt_turn(
    adapter: AcpAdapterCommand,
    initialize: InitializeRequest,
    session_setup: AcpSessionSetupRequest,
    req: AcpPromptTurnRequest,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: PermissionResponderMap,
) -> Result<(), String> {
    let EstablishedSession {
        conn,
        session_id,
        exit_rx,
        runtime_state,
    } = establish_acp_session(
        adapter,
        initialize,
        session_setup,
        req.model.as_deref(),
        &tx,
        &permission_responders,
    )
    .await?;

    // Keep the current session id in runtime state so bridge/session callbacks
    // can resolve it without relying on request-local variables.
    let session_id_str = session_id.0.to_string();
    *runtime_state.current_session_id.borrow_mut() = Some(session_id_str.clone());

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "ACP runtime: session ready (session_id={session_id_str}); sending prompt turn"
            ),
        },
    )
    .await;

    let prompt_blocks: Vec<ContentBlock> = req.prompt.into_iter().map(ContentBlock::from).collect();

    // SIGKILL-FIX: Use `&mut exit_rx` so the select! borrows rather than consumes
    // the receiver.  After the prompt branch completes we explicitly drop `conn`
    // (closes stdin/stdout → signals the adapter to write its session file and
    // exit), then await `exit_rx` with a 10 s timeout.  Awaiting *inside*
    // `run_prompt_turn` keeps the LocalSet alive so its `spawn_local` tasks
    // (including the exit-watcher that holds the child handle) continue running.
    // Without this wait the LocalSet would tear down on `run_prompt_turn` return,
    // the child handle would be dropped, and `kill_on_drop(true)` would SIGKILL
    // the adapter before it could flush the session file.
    let mut exit_rx = exit_rx;
    let prompt_fired = tokio::select! {
        biased;
        prompt_result = conn.prompt(PromptRequest::new(session_id.clone(), prompt_blocks)) => {
            let prompt_response = prompt_result.map_err(|err| err.to_string())?;
            let session = runtime_state
                .current_session_id
                .borrow()
                .clone()
                .unwrap_or_else(|| session_id.0.to_string());
            finalize_successful_turn(
                prompt_response.stop_reason,
                &runtime_state,
                &tx,
                &session,
            )
            .await?;
            true
        }
        // FINDING-14: Sender is dropped on clean exit (code 0), so the receiver
        // sees Err(RecvError) — treat that as a clean shutdown, not a crash.
        // If this happens before a prompt result, it is still a runtime failure:
        // no turn result can ever be emitted.
        exit_msg = &mut exit_rx => {
            if let Ok(msg) = exit_msg {
                return Err(format!("ACP adapter crashed mid-session: {msg}"));
            }
            return Err(
                "ACP adapter exited before returning a prompt result; \
                 verify AXON_ACP_ADAPTER_CMD points to an ACP adapter binary (for example codex-acp)"
                    .to_string(),
            );
        }
    };

    // NOTE: do NOT call permission_responders.clear() here — the map is shared
    // across all concurrent ACP sessions on the same WS connection.  Clearing it
    // drops pending oneshot senders that belong to other in-flight sessions,
    // causing their permission waits to cancel unexpectedly.  Per-session entries
    // are removed by the bridge timeout handler (60 s) or on WS connection drop.

    if prompt_fired {
        // Close connection + wait for adapter to flush session file.
        wait_for_adapter_exit(conn, runtime_state, exit_rx, &session_id_str, &tx).await;
    }

    Ok(())
}

// ── run_session_probe ────────────────────────────────────────────────────────

pub(super) async fn run_session_probe(
    adapter: AcpAdapterCommand,
    initialize: InitializeRequest,
    session_setup: AcpSessionSetupRequest,
    req: AcpSessionProbeRequest,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: PermissionResponderMap,
) -> Result<(), String> {
    let EstablishedSession { exit_rx, .. } = establish_acp_session(
        adapter,
        initialize,
        session_setup,
        req.model.as_deref(),
        &tx,
        &permission_responders,
    )
    .await?;

    // Probe is fire-and-forget: session is set up, we just confirm it worked.
    drop(exit_rx);

    // NOTE: do NOT clear permission_responders — shared across concurrent sessions.

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify AdapterGuard kills the subprocess when dropped.
    /// RAII cleanup is the only mechanism ensuring no subprocess leaks on error
    /// paths — if this test fails, every early return in `spawn_adapter_with_io`
    /// or `establish_acp_session` would leave an orphan process.
    #[tokio::test]
    async fn adapter_guard_kills_process_on_drop() {
        use std::time::Duration;

        // Spawn a real long-running process.
        let child = tokio::process::Command::new("sleep")
            .arg("999")
            .spawn()
            .expect("failed to spawn sleep — is sleep on PATH?");
        let pid = child.id().expect("child has no PID");

        // Wrap in AdapterGuard and immediately drop.
        let guard = AdapterGuard::new(child);
        drop(guard);

        // Give the OS a moment to reap.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify the process no longer exists.
        let proc_path = format!("/proc/{pid}");
        assert!(
            !std::path::Path::new(&proc_path).exists(),
            "AdapterGuard::drop must kill child process (PID {pid} still running)",
        );
    }

    /// Verify that `take()` disarms the guard — dropped guard should NOT kill.
    #[tokio::test]
    async fn adapter_guard_take_disarms() {
        use std::time::Duration;

        let child = tokio::process::Command::new("sleep")
            .arg("999")
            .spawn()
            .expect("failed to spawn sleep");
        let pid = child.id().expect("child has no PID");

        let mut guard = AdapterGuard::new(child);
        let mut taken = guard.take().expect("take should return child");

        // Drop the disarmed guard — must not kill.
        drop(guard);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let proc_path = format!("/proc/{pid}");
        assert!(
            std::path::Path::new(&proc_path).exists(),
            "disarmed AdapterGuard must NOT kill the child process",
        );

        // Clean up: kill the process ourselves.
        let _ = taken.start_kill();
        let _ = taken.wait().await;
    }
}

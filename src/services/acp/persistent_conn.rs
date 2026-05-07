//! Persistent ACP adapter connection handle for WebSocket connections.
//!
//! One `AcpConnectionHandle` per WebSocket connection keeps a single adapter
//! process alive for the WS connection lifetime, dispatching prompt turns via
//! an `mpsc` channel.
//!
//! Adapts Zed's `AcpConnection` pattern for tokio: `ClientSideConnection` is
//! `!Send` (contains `Rc<RefCell<...>>`), so work is dispatched INTO a dedicated
//! `spawn_blocking` thread via channel. Zed stores `_io_task` in the connection
//! struct — here `_join: JoinHandle<()>` plays the same role.

pub(crate) mod editor;
mod session_options;
mod turn;

use std::path::Path;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, ClientSideConnection, CloseSessionRequest, InitializeRequest, SessionId,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::services::events::ServiceEvent;
use crate::services::types::{AcpAdapterCommand, AcpPromptTurnRequest};

use super::runtime::{EstablishedSession, establish_acp_session};
use super::{AcpSessionSetupRequest, PermissionResponderMap};

/// A single prompt turn dispatched to the persistent adapter background thread.
pub struct TurnRequest {
    pub req: AcpPromptTurnRequest,
    pub service_tx: Option<mpsc::Sender<ServiceEvent>>,
    pub result_tx: oneshot::Sender<Result<(), String>>,
}

enum AdapterMessage {
    RunTurn(TurnRequest),
}

/// Handle to a long-lived ACP adapter process for one WebSocket connection.
///
/// Created once on the first `pulse_chat` message; reused for all subsequent
/// turns. Dropping this handle:
/// 1. Cancels `cancel_token` → any in-progress `run_prompt` select! fires the
///    cancel branch, sends `session/cancel` to the adapter, and waits up to 15 s
///    for a graceful `Cancelled` response before the process is killed.
/// 2. Closes `tx` → the background loop's `rx.recv()` returns `None` on the
///    next iteration, so the loop exits cleanly between turns.
/// 3. Drops `_join` → the adapter process is killed via `kill_on_drop(true)`.
///
/// This matches Zed's `Drop for AcpConnection { child.kill() }` semantics,
/// adapted for tokio's `!Send` constraint via channel dispatch.
pub struct AcpConnectionHandle {
    tx: mpsc::Sender<AdapterMessage>,
    /// Cancelled on drop — signals in-progress turns to send `session/cancel`.
    cancel_token: CancellationToken,
    _join: tokio::task::JoinHandle<()>,
}

impl Drop for AcpConnectionHandle {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl AcpConnectionHandle {
    /// Create a no-op handle for testing (no adapter process).
    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        let join = tokio::task::spawn(async {});
        Self {
            tx,
            cancel_token: CancellationToken::new(),
            _join: join,
        }
    }

    /// Spawn the background adapter thread for this WS connection.
    ///
    /// Returns immediately — adapter setup happens on the first `run_turn()`
    /// call so setup progress events appear in the first turn's event stream.
    pub fn spawn(
        adapter: AcpAdapterCommand,
        initialize: InitializeRequest,
        session_setup: AcpSessionSetupRequest,
        permission_responders: PermissionResponderMap,
    ) -> Self {
        let timeout = adapter
            .adapter_timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(std::time::Duration::from_secs(3600)); // Default 1h for persistent

        let cancel_token = CancellationToken::new();
        let loop_cancel = cancel_token.child_token();

        let (tx, rx) = mpsc::channel(16);
        let join = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("[acp_conn] failed to build tokio runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async {
                match tokio::time::timeout(
                    timeout,
                    adapter_loop(
                        adapter,
                        initialize,
                        session_setup,
                        permission_responders,
                        rx,
                        loop_cancel,
                    ),
                )
                .await
                {
                    Ok(_) => {}
                    Err(_) => {
                        tracing::warn!(
                            context = "acp_conn",
                            timeout_secs = timeout.as_secs(),
                            "adapter loop timed out"
                        );
                    }
                }
            });
        });
        Self {
            tx,
            cancel_token,
            _join: join,
        }
    }

    /// Spawn the background adapter thread and begin session establishment eagerly.
    ///
    /// Unlike [`spawn`] which defers setup to the first [`run_turn`] call, this
    /// variant starts `establish_acp_session` immediately so the adapter subprocess
    /// warm-up overlaps with other work (e.g. a Tavily search running concurrently).
    /// Any [`run_turn`] call received while setup is still in progress is queued in
    /// the channel (capacity 16) and executed as soon as the session is ready.
    pub fn spawn_eager(
        adapter: AcpAdapterCommand,
        initialize: InitializeRequest,
        session_setup: AcpSessionSetupRequest,
        model: Option<String>,
        setup_tx: Option<mpsc::Sender<ServiceEvent>>,
        permission_responders: PermissionResponderMap,
    ) -> Self {
        let timeout = adapter
            .adapter_timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(std::time::Duration::from_secs(3600));

        let cancel_token = CancellationToken::new();
        let loop_cancel = cancel_token.child_token();

        let (tx, rx) = mpsc::channel(16);
        let join = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("[acp_conn] failed to build tokio runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async {
                match tokio::time::timeout(
                    timeout,
                    adapter_loop_eager(
                        adapter,
                        initialize,
                        session_setup,
                        model,
                        setup_tx,
                        permission_responders,
                        rx,
                        loop_cancel,
                    ),
                )
                .await
                {
                    Ok(_) => {}
                    Err(_) => {
                        tracing::warn!(
                            context = "acp_conn",
                            timeout_secs = timeout.as_secs(),
                            "adapter loop (eager) timed out"
                        );
                    }
                }
            });
        });
        Self {
            tx,
            cancel_token,
            _join: join,
        }
    }

    /// Dispatch a prompt turn to the background adapter thread.
    ///
    /// Returns `Err` if the channel is closed (adapter exited unexpectedly).
    pub async fn run_turn(&self, turn: TurnRequest) -> Result<(), String> {
        self.tx
            .send(AdapterMessage::RunTurn(turn))
            .await
            .map_err(|_| "ACP adapter channel closed — adapter may have exited".to_string())
    }
}

/// Lazy adapter setup: waits for the first turn, uses its `service_tx` for
/// progress events, then hands off to the shared main loop.
async fn adapter_loop(
    adapter: AcpAdapterCommand,
    initialize: InitializeRequest,
    session_setup: AcpSessionSetupRequest,
    permission_responders: PermissionResponderMap,
    mut rx: mpsc::Receiver<AdapterMessage>,
    cancel_token: CancellationToken,
) {
    let session_cwd = match &session_setup {
        AcpSessionSetupRequest::New(req) => req.cwd.clone(),
        AcpSessionSetupRequest::Load(req) => req.cwd.clone(),
    };

    let first_turn = match rx.recv().await {
        Some(AdapterMessage::RunTurn(t)) => t,
        None => {
            tracing::info!(context = "acp_conn", "channel closed before first turn");
            return;
        }
    };

    let setup_tx = first_turn.service_tx.clone();
    let model = first_turn.req.model.as_deref();

    let setup_result = establish_acp_session(
        adapter,
        initialize,
        session_setup,
        model,
        &setup_tx,
        &permission_responders,
    )
    .await;

    let EstablishedSession {
        mut conn,
        session_id,
        mut exit_rx,
        runtime_state,
    } = match setup_result {
        Ok(s) => {
            tracing::info!(context = "acp_conn", session_id = %s.session_id.0, "adapter ready");
            s
        }
        Err(e) => {
            tracing::error!(context = "acp_conn", error = %e, "adapter setup failed");
            let _ = first_turn
                .result_tx
                .send(Err(format!("ACP adapter setup failed: {e}")));
            return;
        }
    };

    *runtime_state.current_session_id.borrow_mut() = Some(session_id.0.to_string());
    *runtime_state.established_model.borrow_mut() = model.map(str::to_owned);

    // Run the first turn immediately, then enter the shared main loop.
    turn::run_turn_on_conn(
        &mut conn,
        &session_id,
        &session_cwd,
        &runtime_state,
        first_turn,
        &cancel_token,
    )
    .await;

    run_adapter_main_loop(
        &mut conn,
        &session_id,
        &session_cwd,
        &runtime_state,
        &mut exit_rx,
        &mut rx,
        &cancel_token,
    )
    .await;
    tracing::info!(context = "acp_conn", "adapter loop ended");
}

/// Eager adapter setup: establishes the ACP session immediately (before any
/// turn arrives), then enters the shared main loop.
#[allow(clippy::too_many_arguments)]
async fn adapter_loop_eager(
    adapter: AcpAdapterCommand,
    initialize: InitializeRequest,
    session_setup: AcpSessionSetupRequest,
    model: Option<String>,
    setup_tx: Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: PermissionResponderMap,
    mut rx: mpsc::Receiver<AdapterMessage>,
    cancel_token: CancellationToken,
) {
    let session_cwd = match &session_setup {
        AcpSessionSetupRequest::New(req) => req.cwd.clone(),
        AcpSessionSetupRequest::Load(req) => req.cwd.clone(),
    };

    let setup_result = establish_acp_session(
        adapter,
        initialize,
        session_setup,
        model.as_deref(),
        &setup_tx,
        &permission_responders,
    )
    .await;

    let EstablishedSession {
        mut conn,
        session_id,
        mut exit_rx,
        runtime_state,
    } = match setup_result {
        Ok(s) => {
            tracing::info!(context = "acp_conn", session_id = %s.session_id.0, "adapter ready (eager)");
            s
        }
        Err(e) => {
            tracing::error!(context = "acp_conn", error = %e, "adapter setup failed (eager)");
            while let Ok(AdapterMessage::RunTurn(turn)) = rx.try_recv() {
                let _ = turn
                    .result_tx
                    .send(Err(format!("ACP adapter setup failed: {e}")));
            }
            return;
        }
    };

    *runtime_state.current_session_id.borrow_mut() = Some(session_id.0.to_string());
    *runtime_state.established_model.borrow_mut() = model;

    run_adapter_main_loop(
        &mut conn,
        &session_id,
        &session_cwd,
        &runtime_state,
        &mut exit_rx,
        &mut rx,
        &cancel_token,
    )
    .await;
    tracing::info!(context = "acp_conn", "adapter loop ended (eager)");
}

/// Drain the `ClientSideConnection` subscribe stream, logging every JSON-RPC
/// frame at `debug` level for observability (FR-023).
///
/// The task exits automatically when the broadcast sender is dropped (i.e.
/// when the underlying connection closes). Errors from a lagged receiver are
/// logged at warn level and the task continues rather than aborting — the
/// stream is best-effort and must not interfere with normal turn execution.
///
/// This function is called from [`run_adapter_main_loop`] via
/// `tokio::task::spawn_local`, which is valid because:
/// - We are inside a `LocalSet` (created in [`AcpConnectionHandle::spawn`]).
/// - `StreamReceiver` wraps `async_broadcast::Receiver` which is `!Send`.
fn spawn_subscribe_drain(conn: &ClientSideConnection) -> tokio::task::JoinHandle<()> {
    use agent_client_protocol::{StreamMessageContent, StreamMessageDirection};
    let mut receiver = conn.subscribe();
    tokio::task::spawn_local(async move {
        loop {
            let msg = match receiver.recv().await {
                Ok(msg) => msg,
                Err(ref e) => {
                    // `StreamReceiver::recv()` converts `async_broadcast::RecvError`
                    // to an ACP Error via `.data(e.to_string())`.
                    // Overflowed (lagged) produces "receiving skipped N messages";
                    // Closed produces "receiving from an empty and closed channel".
                    let is_lagged = e
                        .data
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .map(|s| s.starts_with("receiving skipped"))
                        .unwrap_or(false);
                    if is_lagged {
                        tracing::warn!(
                            context = "acp_stream",
                            "subscribe drain lagged; some messages dropped — continuing",
                        );
                        continue;
                    }
                    // Channel closed — exit cleanly.
                    break;
                }
            };
            let dir = match msg.direction {
                StreamMessageDirection::Incoming => "←",
                StreamMessageDirection::Outgoing => "→",
            };
            match &msg.message {
                StreamMessageContent::Request { method, .. } => {
                    tracing::debug!(
                        context = "acp_stream",
                        direction = dir,
                        message_type = "request",
                        method = %method,
                    );
                }
                StreamMessageContent::Response { .. } => {
                    tracing::debug!(
                        context = "acp_stream",
                        direction = dir,
                        message_type = "response",
                    );
                }
                StreamMessageContent::Notification { method, .. } => {
                    tracing::debug!(
                        context = "acp_stream",
                        direction = dir,
                        message_type = "notification",
                        method = %method,
                    );
                }
            }
        }
    })
}

/// Shared main loop for both lazy and eager adapter paths.
///
/// Processes [`AdapterMessage::RunTurn`] messages until the channel closes
/// (WS connection dropped) or the adapter process exits unexpectedly.
///
/// Calls [`spawn_subscribe_drain`] on entry to activate the JSON-RPC
/// observability stream (FR-023) for the duration of this connection.
async fn run_adapter_main_loop(
    conn: &mut ClientSideConnection,
    session_id: &SessionId,
    session_cwd: &Path,
    runtime_state: &Arc<super::bridge::AcpRuntimeState>,
    exit_rx: &mut oneshot::Receiver<String>,
    rx: &mut mpsc::Receiver<AdapterMessage>,
    cancel_token: &CancellationToken,
) {
    // Wire the subscribe stream to the debug event bus (FR-023).
    // The drain task owns the receiver and exits when the connection closes.
    let _subscribe_drain = spawn_subscribe_drain(conn);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(AdapterMessage::RunTurn(turn)) => {
                        turn::run_turn_on_conn(
                            conn, session_id, session_cwd, runtime_state, turn, cancel_token,
                        )
                        .await;
                    }
                    None => {
                        tracing::info!(context = "acp_conn", "channel closed (connection ended)");
                        // Best-effort session close before adapter teardown.
                        if runtime_state.close_session_supported.get()
                            && let Err(e) = conn
                                .close_session(CloseSessionRequest::new(session_id.clone()))
                                .await
                        {
                            tracing::warn!(
                                context = "acp_conn",
                                error = %e,
                                "close_session failed (non-fatal)"
                            );
                        }
                        break;
                    }
                }
            }
            exit_result = &mut *exit_rx => {
                match exit_result {
                    Ok(msg) => tracing::error!(context = "acp_conn", message = %msg, "adapter exited unexpectedly"),
                    Err(_) => tracing::info!(context = "acp_conn", "adapter exited cleanly"),
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_run_turn_returns_error_on_closed_channel() {
        let handle = AcpConnectionHandle::dummy();
        let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

        // The dummy handle's receiver is immediately dropped, so the
        // channel is already closed — run_turn should return an error.
        let send_result = handle
            .run_turn(TurnRequest {
                req: AcpPromptTurnRequest {
                    session_id: None,
                    prompt: vec!["test".into()],
                    model: None,
                    session_mode: None,
                    blocked_mcp_tools: vec![],
                    mcp_servers: vec![],
                },
                service_tx: None,
                result_tx,
            })
            .await;

        assert!(send_result.is_err());
        assert!(send_result.unwrap_err().contains("channel closed"),);

        // The result channel should also be closed (no one to receive the turn).
        assert!(result_rx.await.is_err());
    }
}

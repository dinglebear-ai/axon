//! ACP bridge client: implements the `agent_client_protocol::Client` trait,
//! forwarding session notifications and permission requests through the
//! service event channel.

use crate::crates::services::events::{
    EditorOperation, LogLevel, ServiceEvent, emit, emit_nonblocking, emit_with_timeout,
};
use crate::crates::services::types::AcpConfigOption;
use crate::crates::services::types::AcpSessionUpdateKind;
use crate::crates::services::types::{AcpBridgeEvent, AcpTurnResultEvent};
use agent_client_protocol::{
    Client, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, SessionUpdate, StopReason,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::PermissionResponderMap;
use super::mapping::{
    extract_text_delta, map_permission_request_event, map_session_notification_event,
    map_session_update_kind,
};
use super::permission::{auto_approve_outcome, handle_interactive_permission};
use super::persistent_conn::editor::parse_editor_blocks;

// ── Runtime state ───────────────────────────────────────────────────────────

/// FINDING-2: Use RefCell for single-threaded hot path — avoids Mutex lock on
/// every streaming token delta. Safe because the ACP runtime runs on a
/// `current_thread` tokio runtime inside `LocalSet` (all tasks on one thread).
#[derive(Debug, Default)]
pub struct AcpRuntimeState {
    pub(super) current_session_id: std::cell::RefCell<Option<String>>,
    pub(super) assistant_text: std::cell::RefCell<String>,
    /// Current turn's service event sender — updated per-turn by `run_turn_on_conn`
    /// so bridge callbacks (session_notification, request_permission) always route
    /// to the active turn's channel, not the stale first-turn channel.
    pub(super) service_tx: std::cell::RefCell<Option<mpsc::Sender<ServiceEvent>>>,
    /// Monotonically-increasing turn counter.  Incremented at the start of each
    /// turn by `run_turn_on_conn`.  `session_notification` compares the active
    /// turn ID against this value and drops deltas that arrive after the turn has
    /// ended (e.g. late results from a previous timed-out turn).
    pub(super) current_turn_id: std::cell::Cell<u64>,
    /// The model string applied when the ACP session was established.
    /// Updated by `run_turn_on_conn` when a model switch succeeds.
    pub(super) established_model: std::cell::RefCell<Option<String>>,
    /// Latest config options known for this session (from setup and runtime updates).
    /// Used by the persistent runtime to resolve config option IDs for mid-session
    /// config changes (for example model changes).
    pub(super) config_options: std::cell::RefCell<Vec<AcpConfigOption>>,
    /// Disabled MCP tool command names for the active session/runtime.
    pub(super) blocked_mcp_tools: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Overridden timeout for permission requests.
    pub(super) permission_timeout_secs: std::cell::Cell<Option<u64>>,
    /// Guard flag: emit the 1 MiB assistant text warning at most once per session.
    pub(super) limit_warning_emitted: std::cell::Cell<bool>,
}

// ── FIX L-1: return &'static str instead of String ──────────────────────────

pub(super) fn stop_reason_to_str(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        _ => "unknown",
    }
}

// ── Turn finalization (shared by one-shot and persistent paths) ─────────────

/// Finalize a successful prompt turn: log the stop reason, emit `EditorWrite`
/// events for any `<axon:editor>` blocks in the assistant text, and emit the
/// `TurnResult` event.
///
/// Called from both `runtime.rs` (one-shot path) and `persistent_conn/turn.rs`
/// (persistent-connection path) to ensure consistent behavior — especially
/// `EditorWrite` emission, which was previously missing in the one-shot path.
pub(super) async fn finalize_successful_turn(
    stop_reason: StopReason,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    session_id_str: &str,
) -> Result<(), String> {
    let stop_reason_str = stop_reason_to_str(stop_reason);
    let log_level = match stop_reason {
        StopReason::EndTurn => LogLevel::Info,
        StopReason::MaxTokens | StopReason::Refusal | StopReason::Cancelled => LogLevel::Warn,
        _ => LogLevel::Info,
    };
    let msg = format!(
        "ACP runtime: prompt turn completed (stop_reason={stop_reason_str}, session_id={session_id_str})"
    );
    if log_level == LogLevel::Info {
        crate::crates::core::logging::log_info(&msg);
    } else {
        crate::crates::core::logging::log_warn(&msg);
    }
    // Non-blocking emits: turn finalization must never block on a full
    // service event channel — otherwise completed turns hang waiting for
    // the downstream consumer to drain, preventing the caller from
    // receiving the result.
    emit_nonblocking(
        service_tx,
        ServiceEvent::Log {
            level: log_level,
            message: msg,
        },
    );

    let session = session_id_str.to_string();
    let text = runtime_state.assistant_text.borrow().clone();

    for (content, op_str) in parse_editor_blocks(&text) {
        let operation = if op_str == "append" {
            EditorOperation::Append
        } else {
            EditorOperation::Replace
        };
        // Use timeout-guarded send instead of fire-and-forget try_send so that
        // EditorWrite events (which may be the only update for a completed turn)
        // are not silently dropped. 5s timeout caps worst-case per block while
        // still providing backpressure visibility.
        if !emit_with_timeout(
            service_tx,
            ServiceEvent::EditorWrite { content, operation },
            std::time::Duration::from_secs(5),
        )
        .await
        {
            tracing::warn!(
                context = "acp_bridge",
                "EditorWrite event dropped: channel full after 5s timeout",
            );
        }
    }

    let emitted = emit_with_timeout(
        service_tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::TurnResult(AcpTurnResultEvent {
                session_id: session.clone(),
                stop_reason: stop_reason_str.to_string(),
                result: text,
            }),
        },
        std::time::Duration::from_secs(5),
    )
    .await;
    if emitted {
        let msg = format!("ACP runtime: TurnResult emitted (session_id={session})");
        crate::crates::core::logging::log_info(&msg);
        emit_nonblocking(
            service_tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: msg,
            },
        );
    } else {
        crate::crates::core::logging::log_warn(&format!(
            "ACP runtime: TurnResult dropped — channel full after 5s timeout (session_id={session})"
        ));
    }
    Ok(())
}

// ── Bridge client ───────────────────────────────────────────────────────────

/// FINDING-2: `Arc<AcpRuntimeState>` — no Mutex wrapper needed because
/// `AcpRuntimeState` uses `RefCell` internally (not thread-safe).  Cloning
/// `AcpBridgeClient` shares the same state across spawned tasks safely
/// because the ACP runtime runs on a `current_thread` tokio runtime inside
/// a `LocalSet`, guaranteeing single-threaded access.  The `?Send` bound on
/// the `Client` trait impl enforces this at compile time.
#[derive(Clone)]
pub struct AcpBridgeClient {
    pub(super) runtime_state: Arc<AcpRuntimeState>,
    /// When true, permissions are auto-approved without waiting for frontend.
    pub(super) auto_approve: bool,
    /// Pending permission response channels keyed by tool_call_id.
    pub(super) permission_responders: PermissionResponderMap,
}

#[async_trait::async_trait(?Send)]
impl Client for AcpBridgeClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Use the current turn's service_tx. Clone to release borrow before awaits.
        let service_tx = self.runtime_state.service_tx.borrow().clone();
        // Timeout-guarded emit: permission handling must not hang indefinitely
        // when the service event channel is full.  A 5-second timeout lets
        // transient backpressure resolve while guaranteeing forward progress.
        if !emit_with_timeout(
            &service_tx,
            map_permission_request_event(&args),
            std::time::Duration::from_secs(5),
        )
        .await
        {
            tracing::warn!(
                context = "acp_bridge",
                "permission request event dropped: channel full after 5s timeout"
            );
        }

        let tool_call_id = args.tool_call.tool_call_id.0.to_string();
        let tool_name = args
            .tool_call
            .fields
            .title
            .as_deref()
            .unwrap_or_else(|| args.tool_call.tool_call_id.0.as_ref())
            .trim()
            .to_lowercase();

        if !tool_name.is_empty()
            && self
                .runtime_state
                .blocked_mcp_tools
                .borrow()
                .contains(&tool_name)
        {
            emit_nonblocking(
                &service_tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "ACP permission auto-cancelled for blocked MCP tool: {tool_name}"
                    ),
                },
            );
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }

        if self.auto_approve {
            return Ok(RequestPermissionResponse::new(
                auto_approve_outcome(&args, &service_tx, &tool_call_id).await,
            ));
        }

        // Interactive mode: delegate to helper which manages oneshot registration,
        // timeout, and map cleanup.
        let session_id = args.session_id.0.as_ref();
        let outcome = handle_interactive_permission(
            &args,
            &service_tx,
            &self.permission_responders,
            &self.runtime_state,
            session_id,
            &tool_call_id,
        )
        .await;

        Ok(RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        {
            // FINDING-2: RefCell — no Mutex lock on the hot streaming token path.
            // Safe: current_thread runtime + LocalSet ensures single-threaded access.
            let state = &*self.runtime_state;

            // Capture the turn ID at the start of this notification to detect
            // late results from a previous timed-out turn.  `run_turn_on_conn`
            // increments `current_turn_id` before each prompt; if the value changed
            // between when this notification was enqueued and now, we drop the delta.
            let active_turn_id = state.current_turn_id.get();

            if let Some(text_delta) = extract_text_delta(&args.update)
                && matches!(
                    map_session_update_kind(&args.update),
                    AcpSessionUpdateKind::AssistantDelta
                )
            {
                // Reject deltas that arrived after the current turn ended.  This
                // guards against late results from a previous timed-out turn being
                // attributed to the new active turn.
                if active_turn_id != state.current_turn_id.get() {
                    tracing::warn!(
                        context = "acp_bridge",
                        expected_turn_id = active_turn_id,
                        current_turn_id = state.current_turn_id.get(),
                        "dropping late text delta: turn_id mismatch"
                    );
                    return Ok(());
                }
                // Cap at 1 MiB to prevent unbounded accumulation from long sessions.
                const MAX_ASSISTANT_TEXT_BYTES: usize = 1024 * 1024;
                let mut text = state.assistant_text.borrow_mut();
                if text.len() < MAX_ASSISTANT_TEXT_BYTES {
                    text.push_str(&text_delta);
                    if text.len() >= MAX_ASSISTANT_TEXT_BYTES {
                        let msg = format!(
                            "ACP runtime: assistant text reached limit ({MAX_ASSISTANT_TEXT_BYTES} bytes); \
                             further output will be truncated in the final result"
                        );
                        crate::crates::core::logging::log_warn(&msg);
                    }
                }
            }
            *state.current_session_id.borrow_mut() = Some(args.session_id.0.to_string());

            if let SessionUpdate::ConfigOptionUpdate(update) = &args.update {
                let mapped = super::mapping::map_config_options(&update.config_options);
                *state.config_options.borrow_mut() = mapped;
            }
        }

        // Use the current turn's service_tx (updated per-turn by run_turn_on_conn).
        // Clone immediately to release the borrow before the emit call.
        let service_tx = self.runtime_state.service_tx.borrow().clone();

        // Check the text length without holding the borrow across the await.
        let text_at_limit = {
            let text = self.runtime_state.assistant_text.borrow();
            text.len() >= 1024 * 1024 && !text.is_empty()
        };
        if text_at_limit && !self.runtime_state.limit_warning_emitted.get() {
            // Only emit once: the text accumulation block (above) already stops
            // appending past the cap, so the len == cap check is stable.  Guard
            // with a flag so we don't re-emit on every subsequent notification.
            self.runtime_state.limit_warning_emitted.set(true);
            emit(
                &service_tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: "Assistant text limit hit (1 MiB); output truncated".to_string(),
                },
            )
            .await;
        }

        emit(&service_tx, map_session_notification_event(&args)).await;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        PermissionOption, PermissionOptionKind, SessionId, ToolCall, ToolCallId, ToolCallUpdate,
    };

    #[tokio::test]
    async fn test_blocked_mcp_tool_by_name_not_id() {
        let runtime_state = Arc::new(AcpRuntimeState::default());

        // Block "shell" tool.
        runtime_state
            .blocked_mcp_tools
            .borrow_mut()
            .insert("shell".to_string());

        let permission_responders = Arc::new(dashmap::DashMap::new());
        let client = AcpBridgeClient {
            runtime_state: runtime_state.clone(),
            auto_approve: true,
            permission_responders,
        };

        // Create a permission request for "shell" tool with a different ID.
        let tool_call_id = ToolCallId::new("call_123");
        let tool_call = ToolCall::new(tool_call_id, "shell");

        let args = RequestPermissionRequest::new(
            SessionId::new("session-1"),
            ToolCallUpdate::from(tool_call),
            vec![PermissionOption::new(
                "allow",
                "Allow",
                PermissionOptionKind::AllowOnce,
            )],
        );

        let resp = client.request_permission(args).await.unwrap();

        // Tool should be blocked by name ("shell"), not by call ID ("call_123").
        assert_eq!(
            resp.outcome,
            RequestPermissionOutcome::Cancelled,
            "Tool 'shell' should be blocked even if ID is 'call_123'"
        );
    }

    #[tokio::test]
    async fn finalize_emits_editor_write_events() {
        let runtime_state = Arc::new(AcpRuntimeState::default());
        let (tx, mut rx) = mpsc::channel(32);
        let service_tx = Some(tx);

        // Seed assistant text with an editor block.
        *runtime_state.assistant_text.borrow_mut() = concat!(
            "Some preamble.\n",
            "<axon:editor op=\"replace\">\n",
            "# Hello World\n",
            "</axon:editor>\n",
            "trailing text"
        )
        .to_string();

        let result = finalize_successful_turn(
            StopReason::EndTurn,
            &runtime_state,
            &service_tx,
            "test-session-123",
        )
        .await;
        assert!(result.is_ok());

        // Drain events and check for EditorWrite.
        drop(service_tx);
        let mut found_editor_write = false;
        while let Ok(event) = rx.try_recv() {
            if let ServiceEvent::EditorWrite { content, operation } = event {
                assert_eq!(content, "# Hello World");
                assert!(matches!(operation, EditorOperation::Replace));
                found_editor_write = true;
            }
        }
        assert!(found_editor_write, "Expected an EditorWrite event");
    }
}

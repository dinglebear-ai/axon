//! ACP bridge client: implements the `agent_client_protocol::Client` trait,
//! forwarding session notifications and permission requests through the
//! service event channel.

mod state;
pub mod terminal;
pub use state::*;

use crate::crates::services::events::{LogLevel, ServiceEvent, emit, emit_nonblocking};
use crate::crates::services::types::AcpSessionUpdateKind;
use agent_client_protocol::{
    Client, CreateTerminalRequest, CreateTerminalResponse, ReadTextFileRequest,
    ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, TerminalExitStatus, TerminalOutputRequest,
    TerminalOutputResponse, WriteTextFileRequest, WriteTextFileResponse,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::PermissionResponderMap;
use super::mapping::{
    extract_text_delta, map_permission_request_event, map_session_notification_event,
    map_session_update_kind,
};
use super::permission::{auto_approve_outcome, handle_interactive_permission};

// ── Bridge client ───────────────────────────────────────────────────────────

/// `Arc<AcpRuntimeState>` — no Mutex wrapper needed because
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
    /// Working directory for this session — used to validate fs paths from the adapter.
    pub(super) session_cwd: std::path::PathBuf,
    /// Terminal subprocess manager for this session.
    pub(super) terminal_manager: Rc<RefCell<terminal::TerminalManager>>,
}

// ── Path validation ──────────────────────────────────────────────────────────

/// Resolve `path` relative to `cwd` and ensure it stays within `cwd`.
///
/// Normalises `.` and `..` without calling `canonicalize` (so the path need
/// not exist yet, which matters for write requests).  Returns
/// `Err(internal_error)` if the resolved path would escape the working
/// directory.
fn validate_fs_path(
    cwd: &std::path::Path,
    path: &std::path::Path,
) -> agent_client_protocol::Result<std::path::PathBuf> {
    use std::path::Component;

    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    // Normalise both paths by collapsing . and .. without hitting the fs.
    let normalise = |p: &std::path::Path| -> std::path::PathBuf {
        let mut out = std::path::PathBuf::new();
        for c in p.components() {
            match c {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    };

    let resolved = normalise(&base);
    let norm_cwd = normalise(cwd);

    if !resolved.starts_with(&norm_cwd) {
        return Err(agent_client_protocol::Error::internal_error());
    }
    Ok(resolved)
}

#[async_trait::async_trait(?Send)]
impl Client for AcpBridgeClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Use the current turn's service_tx. Clone to release borrow before awaits.
        let service_tx = self.runtime_state.service_tx.borrow().clone();
        // Blocking emit: the permission request event MUST reach the frontend
        // for the interactive approval dialog to appear.  Bound with a 5-second
        // timeout so channel backpressure cannot cause the emit to block
        // indefinitely — which would delay (or bypass) the 60-second permission
        // timeout.  If delivery times out the turn is cancelled immediately.
        if tokio::time::timeout(
            std::time::Duration::from_secs(5),
            emit(&service_tx, map_permission_request_event(&args)),
        )
        .await
        .is_err()
        {
            tracing::warn!(
                context = "acp_bridge",
                "permission request event not delivered within 5s; cancelling turn"
            );
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
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

    async fn read_text_file(
        &self,
        args: ReadTextFileRequest,
    ) -> agent_client_protocol::Result<ReadTextFileResponse> {
        let path = validate_fs_path(&self.session_cwd, &args.path)?;
        let content = tokio::fs::read_to_string(&path).await.map_err(|_| {
            agent_client_protocol::Error::resource_not_found(Some(
                path.to_string_lossy().into_owned(),
            ))
        })?;
        Ok(ReadTextFileResponse::new(content))
    }

    async fn write_text_file(
        &self,
        args: WriteTextFileRequest,
    ) -> agent_client_protocol::Result<WriteTextFileResponse> {
        let path = validate_fs_path(&self.session_cwd, &args.path)?;
        let service_tx = self.runtime_state.service_tx.borrow().clone();
        emit_nonblocking(
            &service_tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: format!("ACP adapter wrote file: {}", path.display()),
            },
        );
        tokio::fs::write(&path, &args.content)
            .await
            .map_err(|_| agent_client_protocol::Error::internal_error())?;
        Ok(WriteTextFileResponse::default())
    }

    async fn create_terminal(
        &self,
        args: CreateTerminalRequest,
    ) -> agent_client_protocol::Result<CreateTerminalResponse> {
        use terminal::DEFAULT_OUTPUT_BYTE_LIMIT;
        let cwd = if let Some(req_cwd) = &args.cwd {
            validate_fs_path(&self.session_cwd, req_cwd)?
        } else {
            self.session_cwd.clone()
        };
        let arg_strs: Vec<&str> = args.args.iter().map(String::as_str).collect();
        let mgr = self.terminal_manager.borrow().clone();
        let local_id = mgr
            .create(&args.command, &arg_strs, &cwd, DEFAULT_OUTPUT_BYTE_LIMIT)
            .await
            .map_err(|_| agent_client_protocol::Error::internal_error())?;
        Ok(CreateTerminalResponse::new(
            agent_client_protocol::TerminalId::new(local_id.0),
        ))
    }

    async fn terminal_output(
        &self,
        args: TerminalOutputRequest,
    ) -> agent_client_protocol::Result<TerminalOutputResponse> {
        let local_id = terminal::TerminalId(args.terminal_id.0.to_string());
        let (text, truncated, exit_code) = self
            .terminal_manager
            .borrow()
            .output(&local_id)
            .map_err(|_| agent_client_protocol::Error::internal_error())?;
        let mut resp = TerminalOutputResponse::new(text, truncated);
        if let Some(code) = exit_code {
            resp.exit_status = Some(TerminalExitStatus::new().exit_code(code as u32));
        }
        Ok(resp)
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        let update_kind = map_session_update_kind(&args.update);

        {
            // RefCell — no Mutex lock on the hot streaming token path.
            // Safe: current_thread runtime + LocalSet ensures single-threaded access.
            let state = &*self.runtime_state;

            if let Some(text_delta) = extract_text_delta(&args.update)
                && matches!(update_kind, AcpSessionUpdateKind::AssistantDelta)
            {
                // Stale-delta protection is handled structurally: `run_turn_on_conn`
                // sets `service_tx` to `None` after each turn completes, so late
                // deltas from a timed-out turn are silently discarded by `emit` /
                // `emit_nonblocking` (both are no-ops when tx is `None`).
                // No turn-ID comparison is needed here — on a single-threaded
                // `LocalSet`, no other task can run between synchronous reads,
                // making an inline guard dead code.

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
            state.apply_config_option_update(&args.update);
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
            self.runtime_state.limit_warning_emitted.set(true);
            emit_nonblocking(
                &service_tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: "Assistant text limit hit (1 MiB); output truncated".to_string(),
                },
            );
        }

        // High-frequency delta variants (AssistantDelta, ThinkingDelta, UserDelta)
        // use emit_nonblocking to avoid backpressure from a slow WS client stalling
        // the adapter subprocess I/O loop.  These are lossy-safe — the complete
        // text is always available in the final TurnResult.
        // All other events (TurnResult, EditorWrite, PermissionRequest, config
        // updates, tool calls) use blocking emit to guarantee delivery.
        let event = map_session_notification_event(&args);
        match update_kind {
            AcpSessionUpdateKind::AssistantDelta
            | AcpSessionUpdateKind::ThinkingDelta
            | AcpSessionUpdateKind::UserDelta => {
                emit_nonblocking(&service_tx, event);
            }
            _ => {
                emit(&service_tx, event).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use crate::crates::services::events::EditorOperation;
    use agent_client_protocol::StopReason;
    use tokio::sync::mpsc;

    #[test]
    fn runtime_state_default_mcp_capabilities_are_false() {
        let state = AcpRuntimeState::default();
        assert!(!state.mcp_http_supported.get());
        assert!(!state.mcp_sse_supported.get());
    }

    #[test]
    fn runtime_state_mcp_capabilities_can_be_set() {
        let state = AcpRuntimeState::default();
        state.mcp_http_supported.set(true);
        state.mcp_sse_supported.set(true);
        assert!(state.mcp_http_supported.get());
        assert!(state.mcp_sse_supported.get());
    }

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
            session_cwd: std::path::PathBuf::new(),
            terminal_manager: Rc::new(RefCell::new(terminal::TerminalManager::new())),
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

//! ACP bridge runtime state, stop-reason helpers, and turn finalization.
//!
//! Extracted from `bridge.rs` to keep the module root under the monolith
//! line limit.  Consumed via `pub(super) use state::*;` in `bridge.rs`.

use std::sync::Arc;

use agent_client_protocol::{SessionUpdate, StopReason};
use tokio::sync::mpsc;

use crate::crates::services::events::{
    EditorOperation, LogLevel, ServiceEvent, emit, emit_nonblocking,
};
use crate::crates::services::types::AcpConfigOption;
use crate::crates::services::types::{AcpBridgeEvent, AcpTurnResultEvent};

use super::super::mapping::map_config_options;
use super::super::persistent_conn::editor::parse_editor_blocks;

// ── Runtime state ───────────────────────────────────────────────────────────

/// Use RefCell for single-threaded hot path — avoids Mutex lock on every
/// streaming token delta. Safe because the ACP runtime runs on a
/// `current_thread` tokio runtime inside `LocalSet` (all tasks on one thread).
#[derive(Debug, Default)]
pub struct AcpRuntimeState {
    pub(crate) current_session_id: std::cell::RefCell<Option<String>>,
    pub(crate) assistant_text: std::cell::RefCell<String>,
    /// Current turn's service event sender — updated per-turn by `run_turn_on_conn`
    /// so bridge callbacks (session_notification, request_permission) always route
    /// to the active turn's channel, not the stale first-turn channel.
    pub(crate) service_tx: std::cell::RefCell<Option<mpsc::Sender<ServiceEvent>>>,
    /// Monotonically-increasing turn counter.  Incremented at the start of each
    /// turn by `run_turn_on_conn`.  `session_notification` compares the active
    /// turn ID against this value and drops deltas that arrive after the turn has
    /// ended (e.g. late results from a previous timed-out turn).
    pub(crate) current_turn_id: std::cell::Cell<u64>,
    /// The model string applied when the ACP session was established.
    /// Updated by `run_turn_on_conn` when a model switch succeeds.
    pub(crate) established_model: std::cell::RefCell<Option<String>>,
    /// Latest config options known for this session (from setup and runtime updates).
    /// Used by the persistent runtime to resolve config option IDs for mid-session
    /// config changes (for example model changes).
    pub(crate) config_options: std::cell::RefCell<Vec<AcpConfigOption>>,
    /// Disabled MCP tool command names for the active session/runtime.
    pub(crate) blocked_mcp_tools: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Overridden timeout for permission requests.
    pub(crate) permission_timeout_secs: std::cell::Cell<Option<u64>>,
    /// Guard flag: emit the 1 MiB assistant text warning at most once per session.
    pub(crate) limit_warning_emitted: std::cell::Cell<bool>,
    /// Current session mode (ask / code / architect). Used to avoid redundant
    /// `set_session_mode` calls when the requested mode matches the active mode.
    pub(crate) current_mode: std::cell::RefCell<Option<String>>,
    /// Whether the adapter advertises HTTP MCP transport support.
    /// Set from `InitializeResponse.agent_capabilities.mcp_capabilities.http`.
    pub(crate) mcp_http_supported: std::cell::Cell<bool>,
    /// Whether the adapter advertises SSE MCP transport support.
    /// Set from `InitializeResponse.agent_capabilities.mcp_capabilities.sse`.
    pub(crate) mcp_sse_supported: std::cell::Cell<bool>,
    /// Whether the adapter supports `session/load`.
    /// Set from `InitializeResponse.agent_capabilities.load_session`.
    pub(crate) load_session_supported: std::cell::Cell<bool>,
    /// Serialized JSON of `PromptCapabilities` from the agent's `InitializeResponse`.
    /// Used by callers that need to inspect what prompt content types the adapter supports.
    pub(crate) prompt_capabilities_json: std::cell::RefCell<Option<String>>,
    /// Whether the adapter advertises `session/close` support.
    /// Set from `InitializeResponse.agent_capabilities.session_capabilities.close`.
    /// Defaults to `true` (assume supported) when the adapter does not advertise
    /// session capabilities (POC-safe assumption; gated at call site).
    pub(crate) close_session_supported: std::cell::Cell<bool>,
}

impl AcpRuntimeState {
    /// Returns the serialized prompt capabilities JSON from the adapter's
    /// `InitializeResponse`, if available.  Callers can inspect this to
    /// determine what content types (image, audio, embedded_context) the
    /// adapter supports before including them in prompt requests.
    pub fn prompt_capabilities(&self) -> Option<String> {
        self.prompt_capabilities_json.borrow().clone()
    }

    /// Apply a `ConfigOptionUpdate` to the runtime state.
    pub(crate) fn apply_config_option_update(&self, update: &SessionUpdate) {
        if let SessionUpdate::ConfigOptionUpdate(u) = update {
            let mapped = map_config_options(&u.config_options);
            *self.config_options.borrow_mut() = mapped;
        }
    }
}

// ── Stop reason → &'static str ──────────────────────────────────────────────

pub fn stop_reason_to_str(reason: StopReason) -> &'static str {
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
pub async fn finalize_successful_turn(
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
    // Log events are fire-and-forget: silently drop if channel is full.
    // EditorWrite and TurnResult below use blocking emit() to guarantee delivery.
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
        // Block until the receiver drains enough space. `emit` returns Err
        // immediately if the receiver is dropped (WS disconnected), so there
        // is no risk of an infinite hang. We must not drop EditorWrite events
        // or TurnResult — they are required for the UI to display the response.
        emit(service_tx, ServiceEvent::EditorWrite { content, operation }).await;
    }

    emit(
        service_tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::TurnResult(AcpTurnResultEvent {
                session_id: session.clone(),
                stop_reason: stop_reason_str.to_string(),
                result: text,
            }),
        },
    )
    .await;
    let msg = format!("ACP runtime: TurnResult dispatch attempted (session_id={session})");
    crate::crates::core::logging::log_info(&msg);
    emit_nonblocking(
        service_tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    );
    Ok(())
}

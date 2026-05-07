//! ACP permission handling: auto-approve logic and interactive frontend
//! permission request flow.

use crate::services::events::{LogLevel, ServiceEvent, emit};
use agent_client_protocol::{
    PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    SelectedPermissionOutcome,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::PermissionResponderMap;
use super::bridge::AcpRuntimeState;

// ── Auto-approve helpers ────────────────────────────────────────────────────

/// Resolve whether ACP permissions should be auto-approved.
///
/// Returns `true` only when `AXON_ACP_AUTO_APPROVE` is explicitly set to a
/// truthy value (`"true"`, `"1"`, or `"yes"`). The default is `false` —
/// unset or any other value means interactive approval is required.
pub(super) fn resolve_acp_auto_approve() -> bool {
    matches!(
        std::env::var("AXON_ACP_AUTO_APPROVE")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Ok("true") | Ok("1") | Ok("yes")
    )
}

/// Select the best auto-approve outcome from the permission request options.
pub(super) async fn auto_approve_outcome(
    args: &RequestPermissionRequest,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    tool_call_id: &str,
) -> RequestPermissionOutcome {
    let outcome = args
        .options
        .iter()
        .find(|opt| matches!(opt.kind, PermissionOptionKind::AllowAlways))
        .or_else(|| {
            args.options
                .iter()
                .find(|opt| matches!(opt.kind, PermissionOptionKind::AllowOnce))
        })
        .map(|opt| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                opt.option_id.clone(),
            ))
        })
        .unwrap_or(RequestPermissionOutcome::Cancelled);

    if matches!(outcome, RequestPermissionOutcome::Selected(_)) {
        let msg = format!("ACP permission auto-approved for tool_call={tool_call_id}");
        crate::core::logging::log_info(&msg);
        emit(
            tx,
            ServiceEvent::Log {
                level: LogLevel::Info,
                message: msg,
            },
        )
        .await;
    }

    outcome
}

// ── Interactive permission helper ───────────────────────────────────────────

/// Wait for the frontend to respond to a permission request.
///
/// Inserts the oneshot sender into `permission_responders`, logs that we are
/// waiting, then races the channel receive against a 60-second timeout.
/// On timeout the map entry is cleaned up before returning `Cancelled`.
///
/// Returns `Cancelled` immediately with a warning if `session_id` is blank —
/// a blank key would prevent the WS router from ever routing the response.
pub(super) async fn handle_interactive_permission(
    args: &RequestPermissionRequest,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    permission_responders: &PermissionResponderMap,
    runtime_state: &Arc<AcpRuntimeState>,
    session_id: &str,
    tool_call_id: &str,
) -> RequestPermissionOutcome {
    // Reject blank session_id early — the composite (session_id, tool_call_id) key
    // would never match a route_permission_response call, so the request would hang
    // until timeout.  Fail fast with a clear error instead.
    if session_id.trim().is_empty() {
        let msg = format!(
            "ACP permission: blank session_id for tool_call={tool_call_id}, \
             cannot route response — cancelling"
        );
        crate::core::logging::log_warn(&msg);
        emit(
            tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: msg,
            },
        )
        .await;
        return RequestPermissionOutcome::Cancelled;
    }

    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<String>();
    let key = (session_id.to_string(), tool_call_id.to_string());
    // Key is (session_id, tool_call_id) — SEC-7: prevents cross-session routing.
    permission_responders.insert(key.clone(), resp_tx);

    // RAII guard: ensures the map entry is removed even if this future is cancelled.
    struct PermissionGuard<'a> {
        map: &'a PermissionResponderMap,
        key: (String, String),
    }
    impl Drop for PermissionGuard<'_> {
        fn drop(&mut self) {
            self.map.remove(&self.key);
        }
    }
    let _guard = PermissionGuard {
        map: permission_responders,
        key,
    };

    let msg = format!("ACP permission awaiting frontend response for tool_call={tool_call_id}");
    crate::core::logging::log_info(&msg);
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: msg,
        },
    )
    .await;

    let timeout_secs = runtime_state.permission_timeout_secs.get().unwrap_or(60);

    // Wait up to N seconds for a response from the frontend.
    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), resp_rx).await;
    resolve_permission_response(args, tx, tool_call_id, result).await
}

/// Map a raw timeout/oneshot result to a `RequestPermissionOutcome` with log events.
///
/// The `PermissionGuard` RAII in the caller handles DashMap entry cleanup on
/// drop, so this function no longer needs access to the map or session_id.
async fn resolve_permission_response(
    args: &RequestPermissionRequest,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    tool_call_id: &str,
    result: Result<
        Result<String, tokio::sync::oneshot::error::RecvError>,
        tokio::time::error::Elapsed,
    >,
) -> RequestPermissionOutcome {
    match result {
        Ok(Ok(option_id)) => {
            let matched = args
                .options
                .iter()
                .find(|opt| *opt.option_id.0 == *option_id);
            match matched {
                Some(opt) => {
                    let msg = format!(
                        "ACP permission resolved by frontend for tool_call={tool_call_id}: {option_id}"
                    );
                    crate::core::logging::log_info(&msg);
                    emit(
                        tx,
                        ServiceEvent::Log {
                            level: LogLevel::Info,
                            message: msg,
                        },
                    )
                    .await;
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        opt.option_id.clone(),
                    ))
                }
                None => {
                    let msg = format!(
                        "ACP permission: frontend sent unknown option_id={option_id} for tool_call={tool_call_id}, cancelling"
                    );
                    crate::core::logging::log_warn(&msg);
                    emit(
                        tx,
                        ServiceEvent::Log {
                            level: LogLevel::Warn,
                            message: msg,
                        },
                    )
                    .await;
                    RequestPermissionOutcome::Cancelled
                }
            }
        }
        // Disconnect: oneshot sender dropped without sending.
        // Note: the PermissionGuard RAII in the caller handles map entry cleanup on drop.
        Ok(Err(_)) => {
            let msg = format!("ACP permission: responder dropped for tool_call={tool_call_id}");
            crate::core::logging::log_warn(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: msg,
                },
            )
            .await;
            RequestPermissionOutcome::Cancelled
        }
        // Timeout: no frontend response within N seconds.
        Err(_) => {
            let msg = format!(
                "ACP permission: timeout waiting for frontend response for tool_call={tool_call_id}"
            );
            crate::core::logging::log_warn(&msg);
            emit(
                tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: msg,
                },
            )
            .await;
            // Note: the PermissionGuard RAII in the caller handles map entry cleanup on drop.
            RequestPermissionOutcome::Cancelled
        }
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        PermissionOption, PermissionOptionKind, SessionId, ToolCall, ToolCallId, ToolCallUpdate,
    };

    fn make_permission_request(session_id: &str, tool_call_id: &str) -> RequestPermissionRequest {
        let tc = ToolCall::new(ToolCallId::new(tool_call_id), "test_tool");
        RequestPermissionRequest::new(
            SessionId::new(session_id),
            ToolCallUpdate::from(tc),
            vec![PermissionOption::new(
                "allow",
                "Allow",
                PermissionOptionKind::AllowOnce,
            )],
        )
    }

    #[tokio::test]
    async fn test_interactive_permission_timeout() {
        let permission_responders: PermissionResponderMap = Arc::new(dashmap::DashMap::new());
        let runtime_state = Arc::new(AcpRuntimeState::default());
        // Set a 1-second timeout so the test completes quickly.
        runtime_state.permission_timeout_secs.set(Some(1));

        let args = make_permission_request("sess-timeout", "tc-1");

        // Don't send any response — let it time out.
        let outcome = handle_interactive_permission(
            &args,
            &None,
            &permission_responders,
            &runtime_state,
            "sess-timeout",
            "tc-1",
        )
        .await;

        assert_eq!(
            outcome,
            RequestPermissionOutcome::Cancelled,
            "Should return Cancelled on timeout"
        );
        // The DashMap entry must be cleaned up after timeout.
        assert!(
            permission_responders.is_empty(),
            "DashMap entry should be cleaned up after timeout"
        );
    }

    #[tokio::test]
    async fn test_interactive_permission_blank_session_id_cancelled() {
        let permission_responders: PermissionResponderMap = Arc::new(dashmap::DashMap::new());
        let runtime_state = Arc::new(AcpRuntimeState::default());
        // Use a long timeout to prove we return immediately, not after waiting.
        runtime_state.permission_timeout_secs.set(Some(300));

        let args = make_permission_request("", "tc-blank");

        let start = std::time::Instant::now();
        let outcome = handle_interactive_permission(
            &args,
            &None,
            &permission_responders,
            &runtime_state,
            "",
            "tc-blank",
        )
        .await;
        let elapsed = start.elapsed();

        assert_eq!(
            outcome,
            RequestPermissionOutcome::Cancelled,
            "Blank session_id should return Cancelled immediately"
        );
        assert!(
            elapsed.as_secs() < 2,
            "Should return immediately, not wait for timeout (took {elapsed:?})"
        );
        assert!(
            permission_responders.is_empty(),
            "No entry should be inserted for blank session_id"
        );
    }
}

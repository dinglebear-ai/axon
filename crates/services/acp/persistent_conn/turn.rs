use std::path::Path;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, CancelNotification, ClientSideConnection, ContentBlock, LoadSessionRequest,
    NewSessionRequest, PromptRequest, SessionId,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::types::AcpBridgeEvent;

use super::super::bridge::{AcpRuntimeState, finalize_successful_turn};
use super::TurnRequest;
use super::session_options::{
    apply_requested_mode_before_prompt, apply_requested_model_before_prompt,
};

struct TurnContext {
    req: crate::crates::services::types::AcpPromptTurnRequest,
    service_tx: Option<mpsc::Sender<ServiceEvent>>,
    result_tx: oneshot::Sender<Result<(), String>>,
    turn_session_id: SessionId,
}

pub(super) async fn run_turn_on_conn(
    conn: &mut ClientSideConnection,
    session_id: &SessionId,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    turn: TurnRequest,
    cancel_token: &CancellationToken,
) {
    prepare_turn_runtime_state(runtime_state);
    let mut turn_ctx = build_turn_context(turn, session_id, runtime_state);

    if let Err(err) = ensure_turn_session(conn, session_cwd, runtime_state, &mut turn_ctx).await {
        let _ = turn_ctx.result_tx.send(Err(err));
        return;
    }

    *runtime_state.current_session_id.borrow_mut() = Some(turn_ctx.turn_session_id.0.to_string());

    if let Err(err) = apply_requested_options(conn, runtime_state, &turn_ctx).await {
        emit(
            &turn_ctx.service_tx,
            ServiceEvent::Log {
                level: LogLevel::Warn,
                message: err,
            },
        )
        .await;
    }

    let prompt_result = run_prompt(conn, runtime_state, &turn_ctx, cancel_token).await;
    let _ = turn_ctx.result_tx.send(prompt_result);
}

fn prepare_turn_runtime_state(runtime_state: &Arc<AcpRuntimeState>) {
    // Increment before touching service channel so stale deltas can be rejected.
    let new_turn_id = runtime_state.current_turn_id.get().wrapping_add(1);
    runtime_state.current_turn_id.set(new_turn_id);
    *runtime_state.assistant_text.borrow_mut() = String::new();
    runtime_state.limit_warning_emitted.set(false);
}

fn build_turn_context(
    turn: TurnRequest,
    session_id: &SessionId,
    runtime_state: &Arc<AcpRuntimeState>,
) -> TurnContext {
    let TurnRequest {
        req,
        service_tx,
        result_tx,
    } = turn;

    *runtime_state.blocked_mcp_tools.borrow_mut() = req
        .blocked_mcp_tools
        .iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect();

    let turn_session_id = current_or_default_session_id(runtime_state, session_id);
    TurnContext {
        req,
        service_tx,
        result_tx,
        turn_session_id,
    }
}

fn current_or_default_session_id(
    runtime_state: &Arc<AcpRuntimeState>,
    session_id: &SessionId,
) -> SessionId {
    let current = runtime_state.current_session_id.borrow().clone();
    SessionId::new(current.as_deref().unwrap_or_else(|| session_id.0.as_ref()))
}

async fn ensure_turn_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    turn_ctx: &mut TurnContext,
) -> Result<(), String> {
    let sdk_servers = super::super::mapping::convert_mcp_servers(&turn_ctx.req.mcp_servers);
    let sdk_servers = super::super::mapping::filter_sdk_mcp_servers(
        &sdk_servers,
        runtime_state.mcp_http_supported.get(),
        runtime_state.mcp_sse_supported.get(),
    );
    let requested = turn_ctx
        .req
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match requested {
        Some(requested_id) => load_or_fallback_session(
            conn,
            session_cwd,
            runtime_state,
            &turn_ctx.service_tx,
            requested_id,
            sdk_servers,
        )
        .await
        .map(|id| turn_ctx.turn_session_id = id),
        None => create_new_session(
            conn,
            session_cwd,
            runtime_state,
            &turn_ctx.service_tx,
            sdk_servers,
        )
        .await
        .map(|id| turn_ctx.turn_session_id = id)
        .map_err(|err| format!("ACP failed to create new session: {err}")),
    }
}

async fn load_or_fallback_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    requested_id: &str,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
) -> Result<SessionId, String> {
    let mut load_req =
        LoadSessionRequest::new(SessionId::new(requested_id), session_cwd.to_path_buf());
    if !mcp_servers.is_empty() {
        load_req = load_req.mcp_servers(mcp_servers.clone());
    }
    let load_result = conn.load_session(load_req).await;

    match load_result {
        Ok(response) => {
            let turn_session_id = SessionId::new(requested_id);
            update_config_options_from_optional(
                runtime_state,
                service_tx,
                &turn_session_id,
                response.config_options,
            )
            .await;
            Ok(turn_session_id)
        }
        Err(err) => {
            emit(
                service_tx,
                ServiceEvent::Log {
                    level: LogLevel::Warn,
                    message: format!(
                        "ACP load_session({requested_id}) failed, creating fallback session. Original session may have uncommitted state. Error: {err}"
                    ),
                },
            )
            .await;
            let fallback =
                create_new_session(conn, session_cwd, runtime_state, service_tx, mcp_servers)
                    .await
                    .map_err(|new_err| {
                        format!(
                            "ACP failed to create fallback session after load failure: {new_err}"
                        )
                    })?;

            emit(
                service_tx,
                ServiceEvent::AcpBridge {
                    event: AcpBridgeEvent::SessionFallback {
                        old_session_id: requested_id.to_string(),
                        new_session_id: fallback.0.to_string(),
                    },
                },
            )
            .await;
            Ok(fallback)
        }
    }
}

async fn create_new_session(
    conn: &mut ClientSideConnection,
    session_cwd: &Path,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    mcp_servers: Vec<agent_client_protocol::McpServer>,
) -> Result<SessionId, String> {
    let mut req = NewSessionRequest::new(session_cwd.to_path_buf());
    if !mcp_servers.is_empty() {
        req = req.mcp_servers(mcp_servers);
    }
    let response = conn.new_session(req).await.map_err(|err| err.to_string())?;

    let turn_session_id = response.session_id;
    update_config_options_from_optional(
        runtime_state,
        service_tx,
        &turn_session_id,
        response.config_options,
    )
    .await;
    Ok(turn_session_id)
}

async fn update_config_options_from_optional(
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    session_id: &SessionId,
    raw_options: Option<Vec<agent_client_protocol::SessionConfigOption>>,
) {
    let Some(raw_options) = raw_options else {
        return;
    };
    let mapped = super::super::mapping::map_config_options(&raw_options);
    if mapped.is_empty() {
        return;
    }

    *runtime_state.config_options.borrow_mut() = mapped.clone();
    emit(
        service_tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ConfigOptionsUpdate {
                session_id: session_id.0.to_string(),
                config_options: mapped,
            },
        },
    )
    .await;
}

async fn apply_requested_options(
    conn: &ClientSideConnection,
    runtime_state: &Arc<AcpRuntimeState>,
    turn_ctx: &TurnContext,
) -> Result<(), String> {
    apply_requested_model_before_prompt(
        conn,
        &turn_ctx.turn_session_id,
        runtime_state,
        turn_ctx.req.model.as_deref(),
        &turn_ctx.service_tx,
    )
    .await
    .map_err(|err| format!("ACP runtime: failed to apply model change mid-session: {err}"))?;

    apply_requested_mode_before_prompt(
        conn,
        &turn_ctx.turn_session_id,
        runtime_state,
        turn_ctx.req.session_mode.as_deref(),
        &turn_ctx.service_tx,
    )
    .await
    .map_err(|err| format!("ACP runtime: failed to apply session_mode mid-session: {err}"))
}

async fn run_prompt(
    conn: &mut ClientSideConnection,
    runtime_state: &Arc<AcpRuntimeState>,
    turn_ctx: &TurnContext,
    cancel_token: &CancellationToken,
) -> Result<(), String> {
    // Early exit if already cancelled — prevents wiring up the prompt future
    // only to have the biased select! immediately pick the cancel branch.
    if cancel_token.is_cancelled() {
        return Err("ACP turn cancelled before prompt started".to_string());
    }

    // Route callbacks for this turn's stream channel.
    *runtime_state.service_tx.borrow_mut() = turn_ctx.service_tx.clone();

    let session_id_str = turn_ctx.turn_session_id.0.to_string();
    emit_prompt_start_log(&turn_ctx.service_tx, &session_id_str).await;

    let prompt_blocks: Vec<ContentBlock> = turn_ctx
        .req
        .prompt
        .iter()
        .cloned()
        .map(ContentBlock::from)
        .collect();

    // Pin the prompt future so we can race it against the cancel token.
    // Both `prompt` and `cancel` take `&self` on `ClientSideConnection`, so
    // concurrent shared borrows are valid — no exclusive borrow conflict.
    let prompt_request = PromptRequest::new(turn_ctx.turn_session_id.clone(), prompt_blocks);
    let prompt_fut = conn.prompt(prompt_request);
    tokio::pin!(prompt_fut);

    let prompt_result: Result<agent_client_protocol::PromptResponse, String> = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => {
            // WS disconnected: send cancel notification to the adapter, then
            // wait up to 15 s for it to return PromptResponse{Cancelled}.
            // Drop the service_tx so no more streaming events are queued.
            *runtime_state.service_tx.borrow_mut() = None;
            // FR-024: `conn.cancel()` sends `session/cancel` (a JSON-RPC
            // notification) which IS the cancellation mechanism defined by
            // the ACP spec.  The SDK 0.10.x does not expose a separate
            // `unstable_cancel_request` method — `cancel()` covers FR-024.
            let _ = conn.cancel(CancelNotification::new(turn_ctx.turn_session_id.clone())).await;
            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                &mut prompt_fut,
            )
            .await
            {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err(
                    "ACP adapter cancel timed out after 15 s; \
                     adapter will be killed when the connection handle drops"
                        .to_string(),
                ),
            }
        }
        result = &mut prompt_fut => result.map_err(|e| e.to_string()),
    };

    // Drop stale events cleanly once prompt completes (may already be None if cancelled).
    *runtime_state.service_tx.borrow_mut() = None;

    match prompt_result {
        Err(e) => Err(e),
        Ok(response) => {
            finalize_successful_turn(
                response.stop_reason,
                runtime_state,
                &turn_ctx.service_tx,
                &session_id_str,
            )
            .await
        }
    }
}

async fn emit_prompt_start_log(service_tx: &Option<mpsc::Sender<ServiceEvent>>, session_id: &str) {
    emit(
        service_tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "ACP runtime: session ready (session_id={session_id}); sending prompt turn"
            ),
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use crate::crates::services::types::AcpMcpServerConfig;

    #[test]
    fn sdk_mcp_servers_from_empty_list_is_empty() {
        let configs: Vec<AcpMcpServerConfig> = vec![];
        let sdk = super::super::super::mapping::convert_mcp_servers(&configs);
        assert!(sdk.is_empty());
    }

    #[test]
    fn sdk_mcp_servers_from_stdio_has_one_entry() {
        let configs = vec![AcpMcpServerConfig::Stdio {
            name: "s".into(),
            command: "/bin/echo".into(),
            args: vec![],
            env: vec![],
        }];
        let sdk = super::super::super::mapping::convert_mcp_servers(&configs);
        assert_eq!(sdk.len(), 1);
    }
}

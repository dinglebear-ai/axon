use std::path::Path;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, ClientSideConnection, ContentBlock, LoadSessionRequest, NewSessionRequest,
    PromptRequest, SessionId,
};
use tokio::sync::{mpsc, oneshot};

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

    let prompt_result = run_prompt(conn, runtime_state, &turn_ctx).await;
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
        )
        .await
        .map(|id| turn_ctx.turn_session_id = id),
        None => create_new_session(conn, session_cwd, runtime_state, &turn_ctx.service_tx)
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
) -> Result<SessionId, String> {
    let load_result = conn
        .load_session(LoadSessionRequest::new(
            SessionId::new(requested_id),
            session_cwd.to_path_buf(),
        ))
        .await;

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
            let fallback = create_new_session(conn, session_cwd, runtime_state, service_tx)
                .await
                .map_err(|new_err| {
                    format!("ACP failed to create fallback session after load failure: {new_err}")
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
) -> Result<SessionId, String> {
    let response = conn
        .new_session(NewSessionRequest::new(session_cwd.to_path_buf()))
        .await
        .map_err(|err| err.to_string())?;

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
) -> Result<(), String> {
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

    let prompt_result = conn
        .prompt(PromptRequest::new(
            turn_ctx.turn_session_id.clone(),
            prompt_blocks,
        ))
        .await;

    // Drop stale events cleanly once prompt completes.
    *runtime_state.service_tx.borrow_mut() = None;

    match prompt_result {
        Err(e) => Err(e.to_string()),
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

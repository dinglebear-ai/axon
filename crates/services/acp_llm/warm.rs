//! Pre-warmed ACP adapter session — overlaps cold-start with other work.

use std::error::Error as StdError;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

use crate::crates::core::config::Config;
use crate::crates::services::acp::{
    AcpClientScaffold, AcpConnectionHandle, PermissionResponderMap, TurnRequest,
};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::{AcpBridgeEvent, AcpPromptTurnRequest, AcpSessionUpdateKind};

use super::runner::{CompletionTurnState, compose_prompt, resolve_adapter_command};
use super::types::{AcpCompletionRequest, AcpCompletionResponse};

/// A pre-warmed ACP adapter connection ready to receive a prompt turn.
///
/// Created by [`warm_session`]; the adapter subprocess starts immediately so its
/// cold-start overlaps with other work (e.g. a Tavily search).
pub struct WarmAcpSession {
    pub(super) handle: AcpConnectionHandle,
}

/// Spawn a fresh warm ACP session without consulting the pool.
///
/// Used internally by the pool to refill without risking circular calls.
/// External callers should prefer [`warm_session`].
pub(super) fn spawn_warm_session(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<WarmAcpSession, Box<dyn StdError>> {
    let adapter = resolve_adapter_command(cfg)?;
    let scaffold = AcpClientScaffold::new(adapter.clone());
    let initialize = scaffold.prepare_initialize()?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // Minimal placeholder — build_session_setup only reads session_id + mcp_servers;
    // a non-empty prompt is required by validate_prompt_turn_request.
    let model = if cfg.openai_model.trim().is_empty() {
        None
    } else {
        Some(cfg.openai_model.clone())
    };
    let dummy_req = AcpPromptTurnRequest {
        session_id: None,
        prompt: vec!["__warm__".to_string()],
        model: model.clone(),
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    };
    let session_setup = scaffold.prepare_session_setup(&dummy_req, &cwd)?;
    let permission_responders: PermissionResponderMap = Arc::new(dashmap::DashMap::new());
    let t = std::time::Instant::now();
    let handle = AcpConnectionHandle::spawn_eager(
        adapter,
        initialize,
        session_setup,
        model,
        tx,
        permission_responders,
    );
    crate::crates::core::logging::log_info(&format!(
        "acp_llm: spawn_eager returned in {}ms (adapter init continues in background)",
        t.elapsed().as_millis()
    ));
    Ok(WarmAcpSession { handle })
}

/// Start warming an ACP adapter session in the background.
///
/// Checks the process-global warm pool first. If the pool has a ready session,
/// returns it immediately (no subprocess spawn). Otherwise falls back to a new
/// one-shot spawn via [`spawn_warm_session`].
///
/// For callers passing an event channel (`tx.is_some()`), pool sessions are
/// bypassed because they were created without event forwarding.
pub fn warm_session(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<WarmAcpSession, Box<dyn StdError>> {
    if tx.is_none()
        && let Some(session) = super::pool::try_checkout(cfg)
    {
        return Ok(session);
    }
    spawn_warm_session(cfg, tx)
}

impl WarmAcpSession {
    /// Send a prompt to the pre-warmed adapter and stream synthesis tokens.
    ///
    /// If the session is still establishing when called, this waits for setup to
    /// complete, then runs the prompt immediately on the warm connection.
    pub async fn complete_streaming<F>(
        self,
        req: AcpCompletionRequest,
        mut on_delta: F,
    ) -> Result<AcpCompletionResponse, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        let prompt_request = AcpPromptTurnRequest {
            session_id: None,
            prompt: vec![compose_prompt(&req)],
            model: req.model.clone(),
            session_mode: None,
            blocked_mcp_tools: vec![],
            mcp_servers: vec![],
        };
        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(64);
        let (result_tx, mut result_rx) = oneshot::channel::<Result<(), String>>();
        let turn = TurnRequest {
            req: prompt_request,
            service_tx: Some(event_tx),
            result_tx,
        };
        self.handle
            .run_turn(turn)
            .await
            .map_err(|e| -> Box<dyn StdError> { e.into() })?;

        let mut state = CompletionTurnState::default();

        // Cap the entire event loop at 300 s so a stalled ACP turn
        // cannot block the caller indefinitely.
        let loop_result = tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                tokio::select! {
                    biased;
                    maybe_event = event_rx.recv() => {
                        match maybe_event {
                            Some(ServiceEvent::AcpBridge { event }) => {
                                match &event {
                                    AcpBridgeEvent::SessionUpdate(update)
                                        if update.kind == AcpSessionUpdateKind::AssistantDelta =>
                                    {
                                        if let Some(delta) = update.text_delta.as_deref() {
                                            on_delta(delta)?;
                                        }
                                    }
                                    AcpBridgeEvent::TurnResult(result) => {
                                        state.text = Some(result.result.clone());
                                    }
                                    _ => {}
                                }
                            }
                            Some(_) => {}
                            // Event channel closed — surface the turn result.
                            None => {
                                match result_rx.try_recv() {
                                    Ok(Ok(())) => {}
                                    Ok(Err(e)) => {
                                        return Err::<(), Box<dyn StdError>>(e.into());
                                    }
                                    Err(_) => {}
                                }
                                break;
                            }
                        }
                    }
                    result = &mut result_rx => {
                        result
                            .map_err(|_| "ACP turn result channel dropped")?
                            .map_err(|e| -> Box<dyn StdError> { e.into() })?;
                        // Drain any events queued after the turn completed.
                        while let Ok(msg) = event_rx.try_recv() {
                            if let ServiceEvent::AcpBridge { event } = msg
                                && let AcpBridgeEvent::TurnResult(result) = &event
                            {
                                state.text = Some(result.result.clone());
                            }
                        }
                        break;
                    }
                }
            }
            Ok(())
        })
        .await;
        match loop_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err("ACP warm session timed out after 300s".into());
            }
        }

        state
            .text
            .map(|text| AcpCompletionResponse {
                text,
                usage: state.usage,
            })
            .ok_or_else(|| "ACP warm session did not emit a turn result".into())
    }

    /// Send a prompt to the pre-warmed adapter and collect the full text response
    /// without streaming. Delegates to `complete_streaming` with a no-op callback.
    pub async fn complete_text(
        self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
        self.complete_streaming(req, |_| Ok(())).await
    }
}

#[cfg(test)]
mod tests {
    use crate::crates::core::config::Config;

    #[test]
    fn warm_session_fails_without_adapter_cmd() {
        let cfg = Config {
            acp_adapter_cmd: None,
            openai_model: "gpt-4o".to_string(),
            ..Config::default()
        };
        let result = super::warm_session(&cfg, None);
        assert!(
            result.is_err(),
            "warm_session must fail when ACP adapter cmd is not set"
        );
    }
}

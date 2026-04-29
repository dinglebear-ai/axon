//! One-shot ACP completion runner — spawns a fresh adapter per request.

use std::error::Error as StdError;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::crates::core::config::Config;
use crate::crates::services::acp::{AcpClientScaffold, PermissionResponderMap};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::{
    AcpAdapterCommand, AcpBridgeEvent, AcpPromptTurnRequest, AcpSessionUpdateKind,
};

use super::types::{
    AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult, AcpUsageSnapshot,
    normalize_stream_flag,
};

const ACP_COMPLETION_TIMEOUT_SECS: u64 = 300;

/// Type-erased delta callback used inside the `current_thread` runtime.
pub(super) type BoxedDeltaFn = Box<dyn FnMut(&str) -> Result<(), Box<dyn StdError>>>;

pub(super) struct AcpRuntimeCompletionRunner {
    pub(super) scaffold: AcpClientScaffold,
}

impl AcpRuntimeCompletionRunner {
    pub(super) fn from_config(cfg: &Config) -> Result<Self, Box<dyn StdError>> {
        Ok(Self {
            scaffold: AcpClientScaffold::new(resolve_adapter_command(cfg)?),
        })
    }

    /// Run a completion turn on a dedicated `current_thread` runtime.
    ///
    /// ACP SDK futures are `!Send` — they cannot run on the multi-threaded
    /// tokio runtime directly.  This mirrors the `run_acp_event_loop` pattern
    /// in `acp.rs`: `spawn_blocking` -> `new_current_thread` -> `LocalSet`.
    pub(super) async fn run_completion_on_blocking_thread(
        &self,
        req: AcpCompletionRequest,
        delta_tx: Option<std::sync::mpsc::Sender<String>>,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
        let scaffold = self.scaffold.clone();
        let timeout = Duration::from_secs(ACP_COMPLETION_TIMEOUT_SECS);

        tracing::debug!(model = ?req.model, streaming = delta_tx.is_some(), "acp_llm: completion started");
        let join = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| format!("failed to create ACP completion tokio runtime: {err}"))?;
            let local = tokio::task::LocalSet::new();
            let on_delta: Option<BoxedDeltaFn> = delta_tx.map(|tx| -> BoxedDeltaFn {
                Box::new(move |delta: &str| {
                    tx.send(delta.to_string())
                        .map_err(|e| -> Box<dyn StdError> { e.to_string().into() })
                })
            });
            local.block_on(&rt, async {
                match tokio::time::timeout(timeout, run_completion_local(scaffold, req, on_delta))
                    .await
                {
                    Ok(result) => result.map_err(|e| e.to_string()),
                    Err(_) => {
                        tracing::warn!(timeout_secs = ACP_COMPLETION_TIMEOUT_SECS, "acp_llm: completion timed out");
                        Err(format!(
                            "ACP completion timed out after {} seconds",
                            ACP_COMPLETION_TIMEOUT_SECS
                        ))
                    }
                }
            })
        })
        .await
        .map_err(|err| -> Box<dyn StdError> {
            tracing::error!(error = %err, "acp_llm: blocking thread panicked or was cancelled");
            format!("failed to join ACP completion worker: {err}").into()
        })?;

        join.map_err(|e| -> Box<dyn StdError> { e.into() })
    }
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for AcpRuntimeCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
        self.run_completion_on_blocking_thread(normalize_stream_flag(req, false), None)
            .await
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        let (delta_tx, delta_rx) = std::sync::mpsc::channel::<String>();
        let completion_handle = self
            .run_completion_on_blocking_thread(normalize_stream_flag(req, true), Some(delta_tx));
        tokio::pin!(completion_handle);

        loop {
            for delta in delta_rx.try_iter() {
                on_delta(&delta)?;
            }
            tokio::select! {
                biased;
                r = &mut completion_handle => {
                    for delta in delta_rx.try_iter() {
                        on_delta(&delta)?;
                    }
                    return r;
                }
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }
}

pub(super) fn resolve_adapter_command(
    cfg: &Config,
) -> Result<AcpAdapterCommand, Box<dyn StdError>> {
    let program = cfg.acp_adapter_cmd.as_deref().unwrap_or("").trim();
    if program.is_empty() {
        tracing::error!("acp_llm: AXON_ACP_ADAPTER_CMD is not set — ACP completions will fail");
        return Err(std::io::Error::other(
            "ACP completion requires AXON_ACP_ADAPTER_CMD to be set",
        )
        .into());
    }

    let args = cfg
        .acp_adapter_args
        .as_deref()
        .map(parse_adapter_args)
        .unwrap_or_default();

    let mut adapter = AcpAdapterCommand::new(program, args);
    adapter.enable_fs = false;
    adapter.enable_terminal = false;
    Ok(adapter)
}

fn parse_adapter_args(raw: &str) -> Vec<String> {
    raw.split('|')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

// ── Internal completion helpers ───────────────────────────────────────────────

async fn run_completion_local(
    scaffold: AcpClientScaffold,
    req: AcpCompletionRequest,
    mut on_delta: Option<BoxedDeltaFn>,
) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
    let prompt_request = AcpPromptTurnRequest {
        session_id: None,
        prompt: vec![compose_prompt(&req)],
        model: req.model.clone(),
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    };
    let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
    let (tx, mut rx) = mpsc::channel::<ServiceEvent>(64);
    let permission_responders: PermissionResponderMap = Arc::new(dashmap::DashMap::new());
    let mut prompt_handle = tokio::task::spawn_local({
        let scaffold = scaffold.clone();
        async move {
            scaffold
                .start_prompt_turn(&prompt_request, cwd, Some(tx), permission_responders)
                .await
        }
    });

    let mut state = CompletionTurnState::default();
    let mut prompt_finished = false;

    loop {
        tokio::select! {
            prompt_result = &mut prompt_handle, if !prompt_finished => {
                prompt_result.map_err(|err| format!("failed to join ACP prompt turn: {err}"))??;
                prompt_finished = true;
            }
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(ServiceEvent::AcpBridge { event }) => {
                        handle_completion_bridge_event(&event, &mut state, &mut on_delta)?;
                    }
                    Some(_) => {}
                    None => {
                        tracing::debug!("acp_llm: event channel closed — adapter finished");
                        break;
                    }
                }
            }
        }
    }

    state
        .text
        .map(|text| AcpCompletionTurnResult {
            text,
            usage: state.usage,
        })
        .ok_or_else(|| {
            tracing::error!("acp_llm: completion finished without a turn result");
            std::io::Error::other("ACP completion runner did not emit a turn result").into()
        })
}

pub(super) fn compose_prompt(req: &AcpCompletionRequest) -> String {
    let user = req.user_prompt.trim();
    match req
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(system) => format!("System instructions:\n{system}\n\nUser request:\n{user}"),
        None => user.to_string(),
    }
}

#[derive(Default)]
pub(super) struct CompletionTurnState {
    pub(super) text: Option<String>,
    pub(super) usage: Option<AcpUsageSnapshot>,
}

pub(super) fn handle_completion_bridge_event(
    event: &AcpBridgeEvent,
    state: &mut CompletionTurnState,
    on_delta: &mut Option<BoxedDeltaFn>,
) -> Result<(), Box<dyn StdError>> {
    match event {
        AcpBridgeEvent::SessionUpdate(update) => {
            if update.kind == AcpSessionUpdateKind::AssistantDelta
                && let Some(delta) = update.text_delta.as_deref()
                && let Some(handler) = on_delta.as_mut()
            {
                handler(delta)?;
            }
        }
        AcpBridgeEvent::UsageUpdate(_) => {}
        AcpBridgeEvent::TurnResult(result) => {
            state.text = Some(result.result.clone());
        }
        _ => {}
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::services::types::{AcpSessionUpdateEvent, AcpSessionUpdateKind};

    #[test]
    fn usage_update_does_not_become_completion_usage_snapshot() {
        let mut state = CompletionTurnState::default();
        let mut on_delta: Option<BoxedDeltaFn> = None;

        handle_completion_bridge_event(
            &AcpBridgeEvent::UsageUpdate(crate::crates::services::types::AcpUsageUpdate {
                session_id: "session-1".to_string(),
                used: 42,
                size: 100,
                cost_amount: None,
                cost_currency: None,
            }),
            &mut state,
            &mut on_delta,
        )
        .expect("usage update should be ignored");

        handle_completion_bridge_event(
            &AcpBridgeEvent::TurnResult(crate::crates::services::types::AcpTurnResultEvent {
                session_id: "session-1".to_string(),
                stop_reason: "end_turn".to_string(),
                result: "final answer".to_string(),
            }),
            &mut state,
            &mut on_delta,
        )
        .expect("turn result should be recorded");

        assert_eq!(state.text.as_deref(), Some("final answer"));
        assert_eq!(state.usage, None);
    }

    #[test]
    fn only_assistant_deltas_are_forwarded_to_stream_callback() {
        let mut state = CompletionTurnState::default();
        let emitted = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let emitted_clone = emitted.clone();
        let mut on_delta: Option<BoxedDeltaFn> = Some(Box::new(
            move |delta: &str| -> Result<(), Box<dyn StdError>> {
                emitted_clone.borrow_mut().push(delta.to_string());
                Ok(())
            },
        ));

        let user_update = AcpSessionUpdateEvent {
            session_id: "session-1".to_string(),
            kind: AcpSessionUpdateKind::UserDelta,
            text_delta: Some("user text".to_string()),
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            tool_content: None,
            tool_input: None,
            tool_locations: None,
            kind_detail: None,
            message_id: None,
        };
        let thinking_update = AcpSessionUpdateEvent {
            session_id: "session-1".to_string(),
            kind: AcpSessionUpdateKind::ThinkingDelta,
            text_delta: Some("thinking text".to_string()),
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            tool_content: None,
            tool_input: None,
            tool_locations: None,
            kind_detail: None,
            message_id: None,
        };
        let assistant_update = AcpSessionUpdateEvent {
            session_id: "session-1".to_string(),
            kind: AcpSessionUpdateKind::AssistantDelta,
            text_delta: Some("assistant text".to_string()),
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            tool_content: None,
            tool_input: None,
            tool_locations: None,
            kind_detail: None,
            message_id: None,
        };

        handle_completion_bridge_event(
            &AcpBridgeEvent::SessionUpdate(user_update),
            &mut state,
            &mut on_delta,
        )
        .expect("user delta should be ignored");
        handle_completion_bridge_event(
            &AcpBridgeEvent::SessionUpdate(thinking_update),
            &mut state,
            &mut on_delta,
        )
        .expect("thinking delta should be ignored");
        handle_completion_bridge_event(
            &AcpBridgeEvent::SessionUpdate(assistant_update),
            &mut state,
            &mut on_delta,
        )
        .expect("assistant delta should be forwarded");

        assert_eq!(*emitted.borrow(), vec!["assistant text".to_string()]);
    }

    /// Regression: `run_completion_on_blocking_thread` must not panic on a
    /// multi-threaded tokio runtime.  Before the fix, `LocalSet::run_until`
    /// was called directly from the multi-threaded runtime, causing:
    /// "LocalSet must be run on a current_thread runtime".
    #[tokio::test]
    async fn run_completion_inner_does_not_panic_on_multi_thread_runtime() {
        let runner = AcpRuntimeCompletionRunner {
            scaffold: AcpClientScaffold::new(AcpAdapterCommand::new(
                "__nonexistent_adapter__",
                Vec::<String>::new(),
            )),
        };
        let req = AcpCompletionRequest::new("hello");
        // Any non-panic result (including an error from the missing adapter)
        // is acceptable.  The test only asserts no panic.
        let _ = runner.run_completion_on_blocking_thread(req, None).await;
    }
}

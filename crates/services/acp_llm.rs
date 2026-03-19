use crate::crates::core::config::Config;
use crate::crates::services::acp::{AcpClientScaffold, PermissionResponderMap};
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::types::{AcpAdapterCommand, AcpBridgeEvent, AcpPromptTurnRequest};
use std::error::Error as StdError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;

const ACP_COMPLETION_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionRequest {
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub model: Option<String>,
    pub stream: bool,
}

impl AcpCompletionRequest {
    #[must_use]
    pub fn new(user_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            user_prompt: user_prompt.into(),
            model: None,
            stream: false,
        }
    }

    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpUsageSnapshot {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl From<agent_client_protocol::Usage> for AcpUsageSnapshot {
    fn from(value: agent_client_protocol::Usage) -> Self {
        Self {
            prompt_tokens: value.input_tokens,
            completion_tokens: value.output_tokens,
            total_tokens: value.total_tokens,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionResponse {
    pub text: String,
    pub usage: Option<AcpUsageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCompletionTurnResult {
    pub text: String,
    pub usage: Option<AcpUsageSnapshot>,
}

#[async_trait::async_trait(?Send)]
pub trait AcpCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>;

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send;
}

#[must_use]
pub fn extract_completion_result(turn_result: AcpCompletionTurnResult) -> AcpCompletionResponse {
    AcpCompletionResponse {
        text: turn_result.text,
        usage: turn_result.usage,
    }
}

pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_text_with_runner(&runner, req).await
}

pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_streaming_with_runner(&runner, req, on_delta).await
}

pub async fn complete_text_with_runner<R>(
    runner: &R,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    R: AcpCompletionRunner + ?Sized,
{
    let turn_result = runner
        .complete_text(normalize_stream_flag(req, false))
        .await?;
    Ok(extract_completion_result(turn_result))
}

pub async fn complete_streaming_with_runner<R, F>(
    runner: &R,
    req: AcpCompletionRequest,
    mut on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    R: AcpCompletionRunner + ?Sized,
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let turn_result = runner
        .complete_streaming(normalize_stream_flag(req, true), &mut on_delta)
        .await?;
    Ok(extract_completion_result(turn_result))
}

struct AcpRuntimeCompletionRunner {
    scaffold: AcpClientScaffold,
}

impl AcpRuntimeCompletionRunner {
    fn from_config(cfg: &Config) -> Result<Self, Box<dyn StdError>> {
        Ok(Self {
            scaffold: AcpClientScaffold::new(resolve_adapter_command(cfg)?),
        })
    }

    async fn run_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
        self.run_completion_inner(req, None::<fn(&str) -> Result<(), Box<dyn StdError>>>)
            .await
    }

    async fn run_completion_inner<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: Option<F>,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        let scaffold = self.scaffold.clone();
        let timeout = Duration::from_secs(ACP_COMPLETION_TIMEOUT_SECS);
        let local = tokio::task::LocalSet::new();
        match tokio::time::timeout(
            timeout,
            local.run_until(run_completion_local(scaffold, req, on_delta)),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(format!(
                "ACP completion timed out after {} seconds",
                ACP_COMPLETION_TIMEOUT_SECS
            )
            .into()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for AcpRuntimeCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
        self.run_text(req).await
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        self.run_completion_inner(req, Some(on_delta)).await
    }
}

fn resolve_adapter_command(cfg: &Config) -> Result<AcpAdapterCommand, Box<dyn StdError>> {
    let program = cfg.acp_adapter_cmd.as_deref().unwrap_or("").trim();
    if program.is_empty() {
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

async fn run_completion_local<F>(
    scaffold: AcpClientScaffold,
    req: AcpCompletionRequest,
    mut on_delta: Option<F>,
) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
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
                    None => break,
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
            std::io::Error::other("ACP completion runner did not emit a turn result").into()
        })
}

fn compose_prompt(req: &AcpCompletionRequest) -> String {
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

fn normalize_stream_flag(mut req: AcpCompletionRequest, stream: bool) -> AcpCompletionRequest {
    req.stream = stream;
    req
}

#[derive(Default)]
struct CompletionTurnState {
    text: Option<String>,
    usage: Option<AcpUsageSnapshot>,
}

fn handle_completion_bridge_event<F>(
    event: &AcpBridgeEvent,
    state: &mut CompletionTurnState,
    on_delta: &mut Option<F>,
) -> Result<(), Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    match event {
        AcpBridgeEvent::SessionUpdate(update) => {
            if let Some(delta) = update.text_delta.as_deref()
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

#[cfg(test)]
mod tests {
    use super::*;

    type DeltaHandler = fn(&str) -> Result<(), Box<dyn StdError>>;

    #[test]
    fn usage_update_does_not_become_completion_usage_snapshot() {
        let mut state = CompletionTurnState::default();
        let mut on_delta: Option<DeltaHandler> = None;

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
}

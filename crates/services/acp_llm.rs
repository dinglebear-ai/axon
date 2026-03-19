use crate::crates::core::config::Config;
use crate::crates::services::acp::AcpClientScaffold;
use crate::crates::services::types::AcpAdapterCommand;
use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, Error, NewSessionRequest, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, SessionUpdate, Usage,
};
use std::cell::RefCell;
use std::error::Error as StdError;
use std::rc::Rc;
use std::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

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

impl From<Usage> for AcpUsageSnapshot {
    fn from(value: Usage) -> Self {
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

#[must_use]
pub fn extract_completion_result(
    text: impl Into<String>,
    usage: Option<AcpUsageSnapshot>,
) -> AcpCompletionResponse {
    AcpCompletionResponse {
        text: text.into(),
        usage,
    }
}

pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    complete_with_runner(cfg, req.stream(false), None).await
}

pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send + 'static,
{
    complete_with_runner(cfg, req.stream(true), Some(Box::new(on_delta))).await
}

type DeltaHandler = Box<dyn FnMut(&str) -> Result<(), Box<dyn StdError>> + Send>;

struct CompletionRunner {
    scaffold: AcpClientScaffold,
}

impl CompletionRunner {
    fn new(scaffold: AcpClientScaffold) -> Self {
        Self { scaffold }
    }

    async fn complete(
        self,
        req: AcpCompletionRequest,
        on_delta: Option<DeltaHandler>,
    ) -> Result<AcpCompletionResponse, String> {
        let scaffold = self.scaffold.clone();
        let req = req.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| format!("failed to create ACP runtime: {err}"))?;
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                match tokio::time::timeout(
                    Duration::from_secs(ACP_COMPLETION_TIMEOUT_SECS),
                    run_completion_local(scaffold, req, on_delta),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(format!(
                        "ACP completion timed out after {} seconds",
                        ACP_COMPLETION_TIMEOUT_SECS
                    )),
                }
            })
        })
        .await
        .map_err(|err| format!("failed to join ACP completion runner: {err}"))?
    }
}

async fn complete_with_runner(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: Option<DeltaHandler>,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    let scaffold = build_scaffold(cfg)?;
    let runner = CompletionRunner::new(scaffold);
    runner
        .complete(req, on_delta)
        .await
        .map_err(|err| Box::new(std::io::Error::other(err)) as Box<dyn StdError>)
}

fn build_scaffold(cfg: &Config) -> Result<AcpClientScaffold, Box<dyn StdError>> {
    let adapter = resolve_adapter_command(cfg)?;
    Ok(AcpClientScaffold::new(adapter))
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

async fn run_completion_local(
    scaffold: AcpClientScaffold,
    req: AcpCompletionRequest,
    on_delta: Option<DeltaHandler>,
) -> Result<AcpCompletionResponse, String> {
    let initialize = scaffold
        .prepare_initialize()
        .map_err(|err| err.to_string())?;
    let child = scaffold
        .spawn_adapter()
        .map_err(|err| format!("failed to spawn ACP adapter: {err}"))?;
    let mut child = child;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "ACP adapter stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ACP adapter stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "ACP adapter stderr unavailable".to_string())?;

    tokio::task::spawn_local(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        tracing::warn!(context = "acp_llm", stderr = %trimmed, "ACP adapter stderr");
                    }
                }
            }
        }
    });

    let state = Rc::new(RefCell::new(CompletionState {
        text: String::new(),
        delta_handler: on_delta,
    }));
    let client = CompletionClient {
        state: Rc::clone(&state),
    };

    let (conn, io_task) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |task| {
            tokio::task::spawn_local(task);
        });

    tokio::task::spawn_local(async move {
        let _ = io_task.await;
    });

    conn.initialize(initialize)
        .await
        .map_err(|err| err.to_string())?;

    let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
    let session_id = conn
        .new_session(NewSessionRequest::new(cwd))
        .await
        .map_err(|err| err.to_string())?
        .session_id;

    let prompt_text = compose_prompt(&req);
    let prompt = PromptRequest::new(session_id, vec![prompt_text.into()]);
    let prompt_response: PromptResponse =
        conn.prompt(prompt).await.map_err(|err| err.to_string())?;

    let text = state.borrow().text.clone();
    let usage = prompt_response.usage.map(AcpUsageSnapshot::from);
    Ok(extract_completion_result(text, usage))
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

struct CompletionState {
    text: String,
    delta_handler: Option<DeltaHandler>,
}

#[derive(Clone)]
struct CompletionClient {
    state: Rc<RefCell<CompletionState>>,
}

#[async_trait::async_trait(?Send)]
impl Client for CompletionClient {
    async fn request_permission(
        &self,
        _args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ))
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        if let Some(delta) = extract_text_delta(&args.update) {
            let mut state = self.state.borrow_mut();
            state.text.push_str(&delta);
            if let Some(handler) = state.delta_handler.as_mut() {
                handler(&delta).map_err(|err| Error::internal_error().data(err.to_string()))?;
            }
        }
        Ok(())
    }
}

fn extract_text_delta(update: &SessionUpdate) -> Option<String> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        },
        _ => None,
    }
}

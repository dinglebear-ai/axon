//! Pool-split protocol helpers: one-time init and per-turn handshakes.
//!
//! These are called by [`crate::core::llm::codex_app_server::pool`] and are
//! separate from the [`super::CodexStreamState`] one-shot state machine.

use serde_json::Value;
use std::error::Error as StdError;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::core::llm::types::{CompletionResponse, LlmBackendConfig, UsageSnapshot};

use super::{
    ID_INITIALIZE, ID_THREAD_START, decline_server_request, initialize_line, parse_usage,
    sanitize_protocol_error, thread_start_lines, turn_start_line,
};

type BoxError = Box<dyn StdError + Send + Sync>;

/// State accumulated during a single `turn/start` cycle against a pooled child.
pub struct CodexTurnState {
    text: String,
    final_item_text: Option<String>,
    usage: Option<UsageSnapshot>,
    last_error: Option<String>,
}

impl CodexTurnState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            final_item_text: None,
            usage: None,
            last_error: None,
        }
    }

    /// Process one server line during a turn cycle.
    pub fn handle_line<F>(&mut self, line: &str, on_delta: &mut F) -> Result<TurnStep, BoxError>
    where
        F: FnMut(&str) -> Result<(), BoxError> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(TurnStep::Continue);
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|err| {
            format!(
                "malformed codex app-server message: {err}: {}",
                sanitize_protocol_error(trimmed)
            )
        })?;
        let method = value.get("method").and_then(Value::as_str);
        let id = value.get("id");
        match (method, id) {
            (Some(_), Some(id)) => Ok(TurnStep::Send(decline_server_request(id).into_lines())),
            (None, Some(_)) => Ok(TurnStep::Continue),
            (Some(method), None) => self.handle_turn_notification(method, &value, on_delta),
            (None, None) => Ok(TurnStep::Continue),
        }
    }

    fn handle_turn_notification<F>(
        &mut self,
        method: &str,
        value: &Value,
        on_delta: &mut F,
    ) -> Result<TurnStep, BoxError>
    where
        F: FnMut(&str) -> Result<(), BoxError> + Send,
    {
        let params = value.get("params");
        match method {
            "item/agentMessage/delta" => {
                if let Some(delta) = params
                    .and_then(|p| p.get("delta"))
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty())
                {
                    self.text.push_str(delta);
                    on_delta(delta)?;
                }
                Ok(TurnStep::Continue)
            }
            "item/completed" => {
                self.capture_completed_item(params);
                Ok(TurnStep::Continue)
            }
            "thread/tokenUsage/updated" => {
                if let Some(usage) = parse_usage(params) {
                    self.usage = Some(usage);
                }
                Ok(TurnStep::Continue)
            }
            "turn/completed" => {
                let turn = params.and_then(|p| p.get("turn"));
                let status = turn.and_then(|t| t.get("status")).and_then(Value::as_str);
                if status == Some("completed") {
                    return Ok(TurnStep::Done);
                }
                let message = turn
                    .and_then(|t| t.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(sanitize_protocol_error)
                    .or_else(|| self.last_error.clone())
                    .unwrap_or_else(|| "turn did not complete successfully".to_string());
                Err(format!("codex app-server turn failed: {message}").into())
            }
            "error" => {
                let message = params
                    .and_then(|p| p.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string();
                let message = sanitize_protocol_error(&message);
                let will_retry = params
                    .and_then(|p| p.get("willRetry"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if will_retry {
                    self.last_error = Some(message);
                    Ok(TurnStep::Continue)
                } else {
                    Err(format!("codex app-server error: {message}").into())
                }
            }
            _ => Ok(TurnStep::Continue),
        }
    }

    fn capture_completed_item(&mut self, params: Option<&Value>) {
        let Some(item) = params.and_then(|p| p.get("item")) else {
            return;
        };
        if item.get("type").and_then(Value::as_str) != Some("agentMessage") {
            return;
        }
        if let Some(text) = item
            .get("text")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
        {
            self.final_item_text = Some(text.to_string());
        }
    }

    pub fn into_response(self) -> Result<CompletionResponse, BoxError> {
        let text = if !self.text.trim().is_empty() {
            self.text
        } else if let Some(text) = self.final_item_text.filter(|t| !t.trim().is_empty()) {
            text
        } else {
            return Err("codex app-server returned no answer text".into());
        };
        Ok(CompletionResponse {
            text,
            usage: self.usage,
        })
    }
}

/// Next action after processing a turn-phase server line.
#[derive(Debug)]
pub enum TurnStep {
    Continue,
    /// Send these lines to the server stdin, then keep reading.
    Send(Vec<String>),
    Done,
}

/// Run the one-time init handshake for a pooled child.
///
/// Sends `initialize`, reads the ack, then sends `initialized` + `thread/start`
/// and returns the `thread_id`.
pub async fn run_init_handshake(
    backend: &LlmBackendConfig,
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
) -> Result<String, BoxError> {
    let version = env!("CARGO_PKG_VERSION");
    let model = backend.codex_model.as_deref();
    let cwd = std::env::temp_dir().display().to_string();

    write_line_async(stdin, &initialize_line(version)).await?;

    let mut lines = stdout.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(trimmed)
                    .map_err(|e| format!("codex pool: malformed init response: {e}: {trimmed}"))?;
                if value.get("id").and_then(Value::as_i64) == Some(ID_INITIALIZE) {
                    if let Some(err) = value.get("error") {
                        return Err(format!(
                            "codex pool: initialize failed: {}",
                            sanitize_protocol_error(&err.to_string())
                        )
                        .into());
                    }
                    break;
                }
            }
            Ok(None) => {
                return Err("codex pool: child closed stdout before initialize ack".into());
            }
            Err(err) => return Err(Box::new(err)),
        }
    }

    for line in &thread_start_lines(model, &cwd, None) {
        write_line_async(stdin, line).await?;
    }

    let mut lines = stdout.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                    format!("codex pool: malformed thread/start response: {e}: {trimmed}")
                })?;
                if value.get("id").and_then(Value::as_i64) == Some(ID_THREAD_START) {
                    if let Some(err) = value.get("error") {
                        return Err(format!(
                            "codex pool: thread/start failed: {}",
                            sanitize_protocol_error(&err.to_string())
                        )
                        .into());
                    }
                    let thread_id = value
                        .get("result")
                        .and_then(|r| r.get("thread"))
                        .and_then(|t| t.get("id"))
                        .and_then(Value::as_str)
                        .ok_or("codex pool: thread/start response missing thread.id")?
                        .to_string();
                    return Ok(thread_id);
                }
            }
            Ok(None) => {
                return Err("codex pool: child closed stdout before thread/start ack".into());
            }
            Err(err) => return Err(Box::new(err)),
        }
    }
}

/// Run one `turn/start` cycle against an already-initialised pooled child.
#[allow(clippy::too_many_arguments)]
pub async fn run_turn_handshake<F>(
    thread_id: &str,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
    _backend: &LlmBackendConfig,
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    on_delta: &mut F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    let _ = model; // model is fixed at pool-spawn time; per-turn override not supported
    write_line_async(stdin, &turn_start_line(thread_id, prompt, effort)).await?;

    let mut state = CodexTurnState::new();
    let mut lines = stdout.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match state.handle_line(&line, on_delta)? {
                TurnStep::Continue => {}
                TurnStep::Send(msgs) => {
                    for msg in &msgs {
                        write_line_async(stdin, msg).await?;
                    }
                }
                TurnStep::Done => return state.into_response(),
            },
            Ok(None) => {
                return Err("codex pool: child closed stdout before turn completed".into());
            }
            Err(err) => return Err(Box::new(err)),
        }
    }
}

async fn write_line_async(stdin: &mut ChildStdin, line: &str) -> Result<(), BoxError> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

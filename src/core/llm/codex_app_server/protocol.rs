//! Wire protocol for `codex app-server` driven as a one-shot synthesis backend.
//!
//! The app server speaks JSON-RPC 2.0-style messages over stdio, but omits the
//! `"jsonrpc": "2.0"` header on the wire. The synthesis handshake is:
//! `initialize` → `initialized` → `thread/start` → `turn/start`, after which the
//! server streams `item/agentMessage/delta` notifications, an `item/completed`
//! carrying the final `agentMessage`, and finally `turn/completed`.
//!
//! [`CodexStreamState`] is a pure state machine: feed it server lines via
//! [`CodexStreamState::handle_line`] and it returns the next [`CodexStep`]
//! (continue reading, send these lines to the server, or finish). Keeping the
//! protocol logic free of process I/O makes it unit-testable without a child.

use serde_json::{Value, json};
use std::error::Error as StdError;

use crate::core::llm::{CompletionResponse, UsageSnapshot};

type BoxError = Box<dyn StdError + Send + Sync>;

/// Client identity reported to the OpenAI Compliance Logs Platform.
const CLIENT_NAME: &str = "axon";
const CLIENT_TITLE: &str = "Axon";

/// JSON-RPC ids for the three requests in the synthesis handshake.
const ID_INITIALIZE: i64 = 0;
const ID_THREAD_START: i64 = 1;
const ID_TURN_START: i64 = 2;
const PROTOCOL_ERROR_LIMIT: usize = 512;

/// The first message every connection must send.
#[must_use]
pub fn initialize_line(version: &str) -> String {
    json!({
        "method": "initialize",
        "id": ID_INITIALIZE,
        "params": {
            "clientInfo": { "name": CLIENT_NAME, "title": CLIENT_TITLE, "version": version },
            "capabilities": Value::Null,
        }
    })
    .to_string()
}

/// `initialized` notification + `thread/start`, sent once the init response lands.
#[must_use]
pub fn thread_start_lines(model: Option<&str>, cwd: &str) -> Vec<String> {
    let mut params = json!({
        "cwd": cwd,
        "approvalPolicy": "never",
        "sandbox": "read-only",
    });
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        params["model"] = json!(model);
    }
    vec![
        json!({ "method": "initialized", "params": {} }).to_string(),
        json!({ "method": "thread/start", "id": ID_THREAD_START, "params": params }).to_string(),
    ]
}

/// `turn/start` carrying the synthesis prompt as a single text input item.
#[must_use]
pub fn turn_start_line(thread_id: &str, prompt: &str) -> String {
    json!({
        "method": "turn/start",
        "id": ID_TURN_START,
        "params": {
            "threadId": thread_id,
            "input": [ { "type": "text", "text": prompt } ],
        }
    })
    .to_string()
}

/// What the orchestrator should do after a server line is processed.
#[derive(Debug, PartialEq, Eq)]
pub enum CodexStep {
    /// Keep reading; nothing to send.
    Continue,
    /// Write each line (newline-framed) to the server's stdin, then keep reading.
    Send(Vec<String>),
    /// The turn completed successfully; stop reading and collect the answer.
    Done,
}

/// Accumulates assistant text + usage across a single `turn/start` lifecycle.
#[derive(Debug, Clone)]
pub struct CodexStreamState {
    model: Option<String>,
    prompt: String,
    cwd: String,
    version: String,
    thread_id: Option<String>,
    text: String,
    final_item_text: Option<String>,
    usage: Option<UsageSnapshot>,
    last_error: Option<String>,
}

impl CodexStreamState {
    #[must_use]
    pub fn new(
        model: Option<String>,
        prompt: impl Into<String>,
        cwd: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            model,
            prompt: prompt.into(),
            cwd: cwd.into(),
            version: version.into(),
            thread_id: None,
            text: String::new(),
            final_item_text: None,
            usage: None,
            last_error: None,
        }
    }

    /// The `initialize` line that kicks off the handshake.
    #[must_use]
    pub fn initial_line(&self) -> String {
        initialize_line(&self.version)
    }

    /// Process one line emitted by the server and decide the next step.
    pub fn handle_line<F>(&mut self, line: &str, on_delta: &mut F) -> Result<CodexStep, BoxError>
    where
        F: FnMut(&str) -> Result<(), BoxError> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(CodexStep::Continue);
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
            // Response to one of our requests (id, no method).
            (None, Some(id)) => self.handle_response(id, &value),
            // Server → client request (method + id): reply with an error so the
            // server does not block waiting on us (should not happen with
            // approvalPolicy="never" + sandbox="read-only", but stay defensive).
            (Some(_), Some(id)) => Ok(decline_server_request(id)),
            // Notification (method, no id).
            (Some(method), None) => self.handle_notification(method, &value, on_delta),
            (None, None) => Ok(CodexStep::Continue),
        }
    }

    fn handle_response(&mut self, id: &Value, value: &Value) -> Result<CodexStep, BoxError> {
        if let Some(err) = value.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .map(sanitize_protocol_error)
                .unwrap_or_else(|| sanitize_protocol_error(&err.to_string()));
            return Err(format!("codex app-server request {id} failed: {message}").into());
        }
        match id.as_i64() {
            Some(ID_INITIALIZE) => Ok(CodexStep::Send(thread_start_lines(
                self.model.as_deref(),
                &self.cwd,
            ))),
            Some(ID_THREAD_START) => {
                let thread_id = value
                    .get("result")
                    .and_then(|r| r.get("thread"))
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_str)
                    .ok_or("codex thread/start response missing thread.id")?
                    .to_string();
                let line = turn_start_line(&thread_id, &self.prompt);
                self.thread_id = Some(thread_id);
                Ok(CodexStep::Send(vec![line]))
            }
            // turn/start ack and any other response: nothing to do.
            _ => Ok(CodexStep::Continue),
        }
    }

    fn handle_notification<F>(
        &mut self,
        method: &str,
        value: &Value,
        on_delta: &mut F,
    ) -> Result<CodexStep, BoxError>
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
                Ok(CodexStep::Continue)
            }
            "item/completed" => {
                self.capture_completed_item(params);
                Ok(CodexStep::Continue)
            }
            "thread/tokenUsage/updated" => {
                if let Some(usage) = parse_usage(params) {
                    self.usage = Some(usage);
                }
                Ok(CodexStep::Continue)
            }
            "turn/completed" => self.handle_turn_completed(params),
            "error" => self.handle_error(params),
            _ => Ok(CodexStep::Continue),
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

    fn handle_turn_completed(&mut self, params: Option<&Value>) -> Result<CodexStep, BoxError> {
        let turn = params.and_then(|p| p.get("turn"));
        let status = turn.and_then(|t| t.get("status")).and_then(Value::as_str);
        if status == Some("completed") {
            return Ok(CodexStep::Done);
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

    fn handle_error(&mut self, params: Option<&Value>) -> Result<CodexStep, BoxError> {
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
            // Codex will retry internally; remember the message in case the turn
            // ultimately fails, but keep reading.
            self.last_error = Some(message);
            Ok(CodexStep::Continue)
        } else {
            Err(format!("codex app-server error: {message}").into())
        }
    }

    /// Collect the final answer text + usage after a successful turn.
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

fn sanitize_protocol_error(text: &str) -> String {
    let mut redacted = crate::core::llm::headless::common::redact_for_error(text);
    if redacted.len() > PROTOCOL_ERROR_LIMIT {
        redacted.truncate(PROTOCOL_ERROR_LIMIT);
        redacted.push_str("...");
    }
    redacted
}

fn decline_server_request(id: &Value) -> CodexStep {
    let reply = json!({
        "id": id,
        "error": {
            "code": -32601,
            "message": "axon synthesis backend does not service requests",
        }
    })
    .to_string();
    CodexStep::Send(vec![reply])
}

fn parse_usage(params: Option<&Value>) -> Option<UsageSnapshot> {
    let total = params?.get("tokenUsage")?.get("total")?;
    let prompt_tokens = total
        .get("inputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let completion_tokens = total
        .get("outputTokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = total
        .get("totalTokens")
        .and_then(Value::as_u64)
        .unwrap_or(prompt_tokens + completion_tokens);
    Some(UsageSnapshot {
        prompt_tokens,
        completion_tokens,
        total_tokens,
    })
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;

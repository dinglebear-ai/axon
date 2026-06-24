use std::io;

use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Child;

pub const STDERR_TAIL_LIMIT: usize = 4096;

const FORBIDDEN_FLAGS: &[&str] = &[
    "--full-auto",
    "--dangerously-bypass-approvals-and-sandbox",
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--yolo",
    "danger-full-access",
    "bypassPermissions",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTransport {
    Stdin,
    Argument,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessCommandSpec {
    pub agent: &'static str,
    pub program: String,
    pub args: Vec<String>,
    pub prompt_transport: PromptTransport,
    pub output_mode: &'static str,
}

impl HeadlessCommandSpec {
    pub fn validate(&self) -> Result<(), String> {
        let joined = self.args.join(" ");
        for forbidden in FORBIDDEN_FLAGS {
            if joined.contains(forbidden) {
                return Err(format!(
                    "headless {} command includes forbidden flag {forbidden}",
                    self.agent
                ));
            }
        }
        // "--yolo" as a standalone flag is forbidden; the value "yolo" in
        // ["--approval-mode", "yolo"] is permitted — it enables native skill
        // activation via activate_skill tool calls in the isolated Gemini home.
        if self.args.iter().any(|arg| arg == "--yolo") {
            return Err(format!(
                "headless {} command includes forbidden --yolo flag",
                self.agent
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessCommandRequest {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

impl HeadlessCommandRequest {
    #[must_use]
    pub fn new(model: Option<String>, system_prompt: Option<String>) -> Self {
        Self {
            model: non_empty(model),
            system_prompt: non_empty(system_prompt),
        }
    }
}

pub fn env_or_default(var_name: &str, default_program: &str) -> String {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_program.to_string())
}

pub fn redacted_stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_LIMIT);
    let text = String::from_utf8_lossy(&stderr[start..]);
    redact_secrets(&text)
}

pub fn append_bounded_tail(buffer: &mut Vec<u8>, chunk: &[u8]) {
    buffer.extend_from_slice(chunk);
    if buffer.len() > STDERR_TAIL_LIMIT {
        let excess = buffer.len() - STDERR_TAIL_LIMIT;
        buffer.drain(..excess);
    }
}

pub(crate) fn joined_prompt(system_prompt: Option<&str>, user_prompt: &str) -> String {
    match system_prompt.map(str::trim).filter(|s| !s.is_empty()) {
        Some(system) => format!("{system}\n\n{user_prompt}"),
        None => user_prompt.to_string(),
    }
}

pub(crate) async fn kill_and_wait(child: &mut Child) -> String {
    let kill_result = child.kill().await;
    let wait_result = child.wait().await;
    match (kill_result, wait_result) {
        (Ok(()), Ok(status)) => format!("killed and reaped with {status}"),
        (Ok(()), Err(wait_err)) => format!("killed but wait failed: {wait_err}"),
        (Err(kill_err), Ok(status)) => format!("kill failed: {kill_err}; wait returned {status}"),
        (Err(kill_err), Err(wait_err)) => {
            format!("kill failed: {kill_err}; wait failed: {wait_err}")
        }
    }
}

pub(crate) async fn read_bounded_stderr(
    stderr: tokio::process::ChildStderr,
) -> Result<Vec<u8>, io::Error> {
    let mut tail = Vec::new();
    let mut reader = BufReader::new(stderr);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok(tail);
        }
        append_bounded_tail(&mut tail, &chunk[..read]);
    }
}

pub(crate) fn redact_for_error(text: &str) -> String {
    redact_secrets(text)
}

/// JSON-aware wrapper around the shared [`crate::redact`] value redactor.
///
/// When `text` parses as JSON, sensitive keys are redacted by name (recursively)
/// and every leaf string value is scrubbed by the shared redactor. Otherwise the
/// whole string is passed through the shared redactor directly.
fn redact_secrets(text: &str) -> String {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return serde_json::to_string(&redact_secret_json(&value))
            .unwrap_or_else(|_| "[REDACTED]".to_string());
    }
    crate::redact::redact_secrets(text)
}

fn redact_secret_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| {
                    if is_sensitive_json_key(key) {
                        (
                            key.clone(),
                            serde_json::Value::String("[REDACTED]".to_string()),
                        )
                    } else {
                        (key.clone(), redact_secret_json(value))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(redact_secret_json).collect())
        }
        serde_json::Value::String(value) => {
            serde_json::Value::String(crate::redact::redact_secrets(value))
        }
        value => value.clone(),
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower == "api_key"
        || lower == "apikey"
        || lower == "token"
        || lower == "access_token"
        || lower == "refresh_token"
        || lower == "authorization"
        || lower == "secret"
        || lower == "client_secret"
        || lower == "password"
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod tests;

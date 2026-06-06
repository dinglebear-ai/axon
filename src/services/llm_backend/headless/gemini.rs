mod home;

use super::common::{
    HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, env_or_default, joined_prompt,
    kill_and_wait, read_bounded_stderr, redacted_stderr_tail,
};
use super::env::apply_env_allowlist;
use crate::services::llm_backend::{CompletionRequest, CompletionResponse, LlmBackendConfig};
use serde_json::Value;
use std::error::Error as StdError;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

const DEFAULT_GEMINI_MODEL: &str = "gemini-3.1-flash-lite-preview";

pub fn build_command(req: &HeadlessCommandRequest) -> Result<HeadlessCommandSpec, String> {
    let args = vec![
        "--prompt".to_string(),
        String::new(),
        "--approval-mode".to_string(),
        "yolo".to_string(),
        "--extensions".to_string(),
        String::new(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--model".to_string(),
        req.model
            .clone()
            .unwrap_or_else(|| DEFAULT_GEMINI_MODEL.to_string()),
    ];
    let spec = HeadlessCommandSpec {
        agent: "gemini",
        program: env_or_default("AXON_HEADLESS_GEMINI_CMD", "gemini"),
        args,
        prompt_transport: PromptTransport::Stdin,
        output_mode: "stream-json",
    };
    spec.validate()?;
    Ok(spec)
}

pub fn validate_command() -> Result<(), Box<dyn StdError + Send + Sync>> {
    let req = HeadlessCommandRequest::new(None, None);
    let spec = build_command(&req)?;
    validate_command_spec(&spec)
}

pub fn validate_config(config: &LlmBackendConfig) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let spec = configured_command_spec(config, None, None)?;
    validate_command_spec(&spec)
}

fn configured_command_spec(
    config: &LlmBackendConfig,
    model: Option<String>,
    system_prompt: Option<String>,
) -> Result<HeadlessCommandSpec, String> {
    let req =
        HeadlessCommandRequest::new(model.or_else(|| config.gemini_model.clone()), system_prompt);
    let mut spec = build_command(&req)?;
    spec.program = config.gemini_cmd.clone();
    Ok(spec)
}

fn validate_command_spec(
    spec: &HeadlessCommandSpec,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    if spec.program.contains('/') || spec.program.contains('\\') {
        let path = Path::new(&spec.program);
        let metadata = fs::symlink_metadata(path)
            .map_err(|err| format!("failed to inspect AXON_HEADLESS_GEMINI_CMD: {err}"))?;
        if metadata.file_type().is_symlink() {
            return Err("AXON_HEADLESS_GEMINI_CMD must not point to a symlink".into());
        }
        if !metadata.is_file() {
            return Err("AXON_HEADLESS_GEMINI_CMD must point to an executable file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err("AXON_HEADLESS_GEMINI_CMD is not executable".into());
            }
        }
    }
    Ok(())
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    mut on_delta: F,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
{
    validate_config(&req.backend)?;
    let spec = configured_command_spec(&req.backend, req.model.clone(), req.system_prompt.clone())?;
    let gemini_home = home::prepare_gemini_home(&req.backend)?;
    let cwd = tempfile::tempdir()
        .map_err(|err| format!("failed to create isolated Gemini cwd: {err}"))?;

    let mut child = spawn_gemini_child(&spec, &gemini_home, cwd.path())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("failed to open Gemini headless stdin")?;
    let prompt = joined_prompt(req.system_prompt.as_deref(), &req.user_prompt);
    let stdin_task = tokio::spawn(async move {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await
    });

    let stdout = child
        .stdout
        .take()
        .ok_or("failed to open Gemini headless stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("failed to open Gemini headless stderr")?;
    let stderr_task = tokio::spawn(async move { read_bounded_stderr(stderr).await });

    let timeout = req.backend.completion_timeout();
    let mut parser = GeminiStreamState::default();
    let mut lines = BufReader::new(stdout).lines();
    let stream_result = match tokio::time::timeout(timeout, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if let Err(err) = parser.handle_line(&line, &mut on_delta) {
                        break Err(err);
                    }
                }
                Ok(None) => break Ok(()),
                Err(err) => break Err(Box::new(err) as Box<dyn StdError + Send + Sync>),
            }
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            let cleanup = kill_and_wait(&mut child).await;
            stdin_task.abort();
            stderr_task.abort();
            let _ = stdin_task.await;
            let _ = stderr_task.await;
            return Err(format!(
                "Gemini headless timed out after {} seconds; cleanup: {cleanup}",
                timeout.as_secs(),
            )
            .into());
        }
    };
    if let Err(err) = stream_result {
        let cleanup = kill_and_wait(&mut child).await;
        let _ = stdin_task.await;
        let _ = stderr_task.await;
        return Err(format!("{err}; cleanup: {cleanup}").into());
    }

    stdin_task
        .await
        .map_err(|err| format!("failed to join Gemini stdin writer: {err}"))??;
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            let cleanup = kill_and_wait(&mut child).await;
            stderr_task.abort();
            let _ = stderr_task.await;
            return Err(format!(
                "Gemini headless timed out waiting for process exit after {} seconds; cleanup: {cleanup}",
                timeout.as_secs(),
            )
            .into());
        }
    };
    let stderr = match tokio::time::timeout(timeout, stderr_task).await {
        Ok(joined) => {
            joined.map_err(|err| format!("failed to join Gemini stderr reader: {err}"))??
        }
        Err(_) => {
            return Err(format!(
                "Gemini headless timed out reading stderr after {} seconds",
                timeout.as_secs()
            )
            .into());
        }
    };

    if !status.success() {
        return Err(format!(
            "Gemini headless exited with {status}; stderr: {}",
            redacted_stderr_tail(&stderr)
        )
        .into());
    }

    let text = parser.finish()?;
    Ok(CompletionResponse { text, usage: None })
}

fn spawn_gemini_child(
    spec: &HeadlessCommandSpec,
    gemini_home: &TempDir,
    cwd: &Path,
) -> Result<Child, Box<dyn StdError + Send + Sync>> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_env_allowlist(&mut command);
    command
        .env("HOME", gemini_home.path())
        .env("XDG_CONFIG_HOME", gemini_home.path().join(".config"))
        .env("XDG_CACHE_HOME", gemini_home.path().join(".cache"))
        // Gemini 0.41+ requires workspace trust for headless/non-interactive use.
        .env("GEMINI_CLI_TRUST_WORKSPACE", "true");

    command
        .spawn()
        .map_err(|err| format!("failed to spawn Gemini headless command: {err}").into())
}

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    complete_streaming(req, |_| Ok(())).await
}

#[derive(Default)]
struct GeminiStreamState {
    text: String,
    result_text: Option<String>,
    saw_success: bool,
}

impl GeminiStreamState {
    fn handle_line<F>(
        &mut self,
        line: &str,
        on_delta: &mut F,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|err| format!("malformed Gemini stream JSON: {err}: {trimmed}"))?;
        match value.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                // Permitted tool calls for synthesis mode:
                // - "activate_skill": loads the axon-rag-synthesize skill (intentional)
                // - "update_topic": Gemini 0.41.2+ internal session management (harmless)
                // All other tool_use events indicate unexpected tool execution and are rejected.
                // Field name changed from "name" to "tool_name" in Gemini CLI 0.41.2.
                let tool_name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("tool_name").and_then(Value::as_str));
                let permitted = matches!(tool_name, Some("activate_skill") | Some("update_topic"));
                if !permitted {
                    return Err(format!(
                        "Gemini headless emitted unexpected tool call '{}' in synthesis mode; raw event: {value}",
                        tool_name.unwrap_or("unknown")
                    )
                    .into());
                }
            }
            Some("tool_result") => {
                // tool_result from activate_skill — skill content injected into context.
                // No text to accumulate; continue to collect the model's final response.
            }
            Some("error") => {
                return Err(format!("Gemini headless stream error: {value}").into());
            }
            Some("message") => {
                if value.get("role").and_then(Value::as_str) == Some("assistant")
                    && let Some(delta) = message_content(&value)
                {
                    self.push_delta(&delta, on_delta)?;
                }
            }
            Some("result") => self.handle_result(&value)?,
            _ if contains_tool_event(&value) => {
                return Err("Gemini headless emitted a tool event in synthesis-only mode".into());
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_result(&mut self, value: &Value) -> Result<(), Box<dyn StdError + Send + Sync>> {
        if value.get("status").and_then(Value::as_str) != Some("success") {
            return Err(format!("Gemini headless returned unsuccessful result: {value}").into());
        }
        // Per-event whitelist (activate_skill, update_topic) is the primary defence.
        // The stats tool_calls count is no longer used as a secondary gate — Gemini
        // 0.41.2+ calls update_topic automatically, making the count unreliable.
        if let Some(text) = value.get("response").and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            self.result_text = Some(text.to_string());
        }
        self.saw_success = true;
        Ok(())
    }

    fn push_delta<F>(
        &mut self,
        delta: &str,
        on_delta: &mut F,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
    {
        if delta.is_empty() {
            return Ok(());
        }
        self.text.push_str(delta);
        on_delta(delta)
    }

    fn finish(self) -> Result<String, Box<dyn StdError + Send + Sync>> {
        if !self.saw_success {
            return Err("Gemini headless stream ended without a success result".into());
        }
        if !self.text.trim().is_empty() {
            return Ok(self.text);
        }
        if let Some(result) = self.result_text
            && !result.trim().is_empty()
        {
            return Ok(result);
        }
        Err("Gemini headless returned no answer text".into())
    }
}

fn message_content(value: &Value) -> Option<String> {
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return Some(content.to_string());
    }
    if let Some(parts) = value.get("content").and_then(Value::as_array) {
        let mut out = String::new();
        for part in parts {
            if let Some(text) = part.as_str() {
                out.push_str(text);
            } else if let Some(text) = part.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
        return (!out.is_empty()).then_some(out);
    }
    None
}

fn contains_tool_event(value: &Value) -> bool {
    match value {
        Value::String(s) => matches!(s.as_str(), "tool_use" | "tool_result"),
        Value::Array(items) => items.iter().any(contains_tool_event),
        Value::Object(map) => map.iter().any(|(key, value)| {
            key == "tool_use"
                || key == "tool_result"
                || (key == "type"
                    && value
                        .as_str()
                        .is_some_and(|s| s == "tool_use" || s == "tool_result"))
                || contains_tool_event(value)
        }),
        _ => false,
    }
}

#[cfg(test)]
fn assemble_utf8_chunks(chunks: &[&[u8]]) -> Result<String, std::str::Utf8Error> {
    let bytes = chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect::<Vec<_>>();
    std::str::from_utf8(&bytes).map(ToString::to_string)
}

#[cfg(test)]
#[path = "gemini_tests.rs"]
mod tests;

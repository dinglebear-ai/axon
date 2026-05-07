use super::HeadlessAgent;
use super::common::{
    HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, append_bounded_tail,
    env_or_default, redacted_stderr_tail,
};
use crate::services::acp::apply_env_allowlist;
use crate::services::acp_llm::{AcpCompletionRequest, AcpCompletionResponse};
use serde_json::{Value, json};
use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const DEFAULT_GEMINI_MODEL: &str = "gemini-3.1-flash-lite-preview";
const GEMINI_AUTH_FILES: &[&str] = &[
    "oauth_creds.json",
    "gemini-credentials.json",
    "google_accounts.json",
];

pub fn build_command(req: &HeadlessCommandRequest) -> Result<HeadlessCommandSpec, String> {
    let args = vec![
        "--prompt".to_string(),
        String::new(),
        "--approval-mode".to_string(),
        "plan".to_string(),
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
        agent: HeadlessAgent::Gemini,
        program: env_or_default("AXON_HEADLESS_GEMINI_CMD", "gemini"),
        args,
        prompt_transport: PromptTransport::Stdin,
        output_mode: "stream-json",
    };
    spec.validate()?;
    Ok(spec)
}

pub fn safe_posture_available() -> bool {
    true
}

pub async fn complete_streaming<F>(
    req: AcpCompletionRequest,
    mut on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let command_req = HeadlessCommandRequest::new(req.model.clone(), req.system_prompt.clone());
    let spec = build_command(&command_req)?;
    let gemini_home = prepare_gemini_home()?;
    let cwd = tempfile::tempdir()
        .map_err(|err| format!("failed to create isolated Gemini cwd: {err}"))?;

    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(cwd.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_env_allowlist(&mut command);
    command
        .env("HOME", gemini_home.path())
        .env("XDG_CONFIG_HOME", gemini_home.path().join(".config"))
        .env("XDG_CACHE_HOME", gemini_home.path().join(".cache"));

    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to spawn Gemini headless command: {err}"))?;

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

    let mut parser = GeminiStreamState::default();
    let mut lines = BufReader::new(stdout).lines();
    let stream_result: Result<(), Box<dyn StdError>> = loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if let Err(err) = parser.handle_line(&line, &mut on_delta) {
                    break Err(err);
                }
            }
            Ok(None) => break Ok(()),
            Err(err) => break Err(Box::new(err) as Box<dyn StdError>),
        }
    };
    if let Err(err) = stream_result {
        let _ = child.kill().await;
        let _ = child.wait().await;
        let _ = stdin_task.await;
        let _ = stderr_task.await;
        return Err(err);
    }

    stdin_task
        .await
        .map_err(|err| format!("failed to join Gemini stdin writer: {err}"))??;
    let status = child.wait().await?;
    let stderr = stderr_task
        .await
        .map_err(|err| format!("failed to join Gemini stderr reader: {err}"))??;

    if !status.success() {
        return Err(format!(
            "Gemini headless exited with {status}; stderr: {}",
            redacted_stderr_tail(&stderr)
        )
        .into());
    }

    let text = parser.finish()?;
    Ok(AcpCompletionResponse { text, usage: None })
}

fn joined_prompt(system_prompt: Option<&str>, user_prompt: &str) -> String {
    match system_prompt.map(str::trim).filter(|s| !s.is_empty()) {
        Some(system) => format!("{system}\n\n{user_prompt}"),
        None => user_prompt.to_string(),
    }
}

async fn read_bounded_stderr(
    stderr: tokio::process::ChildStderr,
) -> Result<Vec<u8>, std::io::Error> {
    let mut tail = Vec::new();
    let mut reader = BufReader::new(stderr);
    let mut chunk = [0u8; 1024];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok(tail);
        }
        append_bounded_tail(&mut tail, &chunk[..read]);
    }
}

fn prepare_gemini_home() -> Result<TempDir, Box<dyn StdError>> {
    let temp = tempfile::Builder::new()
        .prefix("axon-gemini-headless-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated Gemini HOME: {err}"))?;
    let gemini_dir = temp.path().join(".gemini");
    fs::create_dir_all(&gemini_dir)?;
    fs::create_dir_all(temp.path().join(".config"))?;
    fs::create_dir_all(temp.path().join(".cache"))?;

    let source_home = gemini_source_home()?;
    let source_gemini = source_home.join(".gemini");
    for filename in GEMINI_AUTH_FILES {
        let src = source_gemini.join(filename);
        if src.is_file() {
            fs::copy(&src, gemini_dir.join(filename)).map_err(|err| {
                format!("failed to copy Gemini auth file {}: {err}", src.display())
            })?;
        }
    }

    write_isolated_settings(&gemini_dir.join("settings.json"))?;
    Ok(temp)
}

fn gemini_source_home() -> Result<PathBuf, Box<dyn StdError>> {
    if let Some(path) = non_empty_env("AXON_HEADLESS_GEMINI_HOME") {
        return Ok(PathBuf::from(path));
    }
    non_empty_env("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is required to locate Gemini CLI auth files".into())
}

fn non_empty_env(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn write_isolated_settings(path: &Path) -> Result<(), Box<dyn StdError>> {
    let settings = json!({
        "admin": {
            "mcp": { "enabled": false },
            "extensions": { "enabled": false },
            "skills": { "enabled": false }
        },
        "mcpServers": {},
        "hooks": {},
        "context": { "fileName": [] },
        "security": { "auth": { "selectedType": "oauth-personal" } }
    });
    fs::write(path, serde_json::to_vec_pretty(&settings)?)?;
    Ok(())
}

#[derive(Default)]
struct GeminiStreamState {
    text: String,
    result_text: Option<String>,
    saw_success: bool,
}

impl GeminiStreamState {
    fn handle_line<F>(&mut self, line: &str, on_delta: &mut F) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|err| format!("malformed Gemini stream JSON: {err}: {trimmed}"))?;
        match value.get("type").and_then(Value::as_str) {
            Some("tool_use" | "tool_result") => {
                return Err("Gemini headless emitted a tool event in synthesis-only mode".into());
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

    fn handle_result(&mut self, value: &Value) -> Result<(), Box<dyn StdError>> {
        if value.get("status").and_then(Value::as_str) != Some("success") {
            return Err(format!("Gemini headless returned unsuccessful result: {value}").into());
        }
        if value
            .pointer("/stats/tool_calls")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0)
        {
            return Err("Gemini headless reported tool calls in synthesis-only mode".into());
        }
        if let Some(text) = value.get("response").and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            self.result_text = Some(text.to_string());
        }
        self.saw_success = true;
        Ok(())
    }

    fn push_delta<F>(&mut self, delta: &str, on_delta: &mut F) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        if delta.is_empty() {
            return Ok(());
        }
        self.text.push_str(delta);
        on_delta(delta)
    }

    fn finish(self) -> Result<String, Box<dyn StdError>> {
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
mod tests {
    use super::*;

    #[test]
    fn gemini_headless_command_avoids_yolo() {
        let spec = build_command(&HeadlessCommandRequest::new(
            None,
            Some("system".to_string()),
        ))
        .unwrap();
        let joined = spec.args.join(" ");
        assert_eq!(spec.prompt_transport, PromptTransport::Stdin);
        assert!(joined.contains("--approval-mode plan"));
        assert!(spec.args.windows(2).any(|w| w == ["--extensions", ""]));
        assert!(joined.contains("--model gemini-3.1-flash-lite-preview"));
        assert!(!joined.contains("--yolo"));
        assert!(!joined.contains("--approval-mode=yolo"));
    }

    #[test]
    fn gemini_headless_command_honors_model_override() {
        let spec = build_command(&HeadlessCommandRequest::new(
            Some("gemini-3.1-pro-preview".to_string()),
            None,
        ))
        .unwrap();
        assert!(
            spec.args
                .windows(2)
                .any(|w| w == ["--model", "gemini-3.1-pro-preview"])
        );
    }

    #[test]
    fn gemini_headless_parser_streams_message_content() {
        let mut state = GeminiStreamState::default();
        let mut out = String::new();
        state
            .handle_line(
                r#"{"type":"message","role":"assistant","content":"hel","delta":true}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        state
            .handle_line(
                r#"{"type":"message","role":"assistant","content":"lo","delta":true}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        state
            .handle_line(
                r#"{"type":"result","status":"success","stats":{"tool_calls":0}}"#,
                &mut |_| Ok(()),
            )
            .unwrap();
        assert_eq!(out, "hello");
        assert_eq!(state.finish().unwrap(), "hello");
    }

    #[test]
    fn gemini_headless_parser_rejects_tool_events() {
        let mut state = GeminiStreamState::default();
        let err = state
            .handle_line(r#"{"type":"tool_use","name":"shell"}"#, &mut |_| Ok(()))
            .expect_err("tool events must fail closed");
        assert!(err.to_string().contains("tool event"));
    }

    #[test]
    fn gemini_headless_parser_rejects_reported_tool_calls() {
        let mut state = GeminiStreamState::default();
        let err = state
            .handle_line(
                r#"{"type":"result","status":"success","stats":{"tool_calls":1}}"#,
                &mut |_| Ok(()),
            )
            .expect_err("reported tool calls must fail closed");
        assert!(err.to_string().contains("tool calls"));
    }

    #[test]
    fn gemini_headless_assembles_chunked_stdout() {
        let out = assemble_utf8_chunks(&[b"hello ", b"world"]).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn gemini_headless_assembles_split_multibyte_codepoint() {
        let snowman = "hi \u{2603}".as_bytes();
        let out = assemble_utf8_chunks(&[&snowman[..4], &snowman[4..]]).unwrap();
        assert_eq!(out, "hi \u{2603}");
    }
}

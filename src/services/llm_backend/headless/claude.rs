use super::HeadlessAgent;
use super::common::{
    HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, append_bounded_tail,
    env_or_default, redacted_stderr_tail,
};
use crate::services::acp::apply_env_allowlist;
use crate::services::acp_llm::{AcpCompletionRequest, AcpCompletionResponse};
use serde_json::Value;
use std::error::Error as StdError;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

pub fn build_command(req: &HeadlessCommandRequest) -> Result<HeadlessCommandSpec, String> {
    let mut args = vec![
        "-p".to_string(),
        "--input-format".to_string(),
        "text".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--include-partial-messages".to_string(),
        "--no-session-persistence".to_string(),
        "--permission-mode".to_string(),
        "plan".to_string(),
        "--tools".to_string(),
        String::new(),
        "--strict-mcp-config".to_string(),
        "--mcp-config".to_string(),
        "{\"mcpServers\":{}}".to_string(),
    ];
    if let Some(model) = req.model.as_ref() {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(system_prompt) = req.system_prompt.as_ref() {
        args.extend(["--system-prompt".to_string(), system_prompt.clone()]);
    }
    let spec = HeadlessCommandSpec {
        agent: HeadlessAgent::Claude,
        program: env_or_default("AXON_HEADLESS_CLAUDE_CMD", "claude"),
        args,
        prompt_transport: PromptTransport::Stdin,
        output_mode: "stream-json",
    };
    spec.validate()?;
    Ok(spec)
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
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_env_allowlist(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to spawn Claude headless command: {err}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or("failed to open Claude headless stdin")?;
    let prompt = req.user_prompt.clone();
    let stdin_task = tokio::spawn(async move {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await
    });

    let stdout = child
        .stdout
        .take()
        .ok_or("failed to open Claude headless stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("failed to open Claude headless stderr")?;
    let stderr_task = tokio::spawn(async move { read_bounded_stderr(stderr).await });

    let mut parser = ClaudeStreamState::default();
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
        .map_err(|err| format!("failed to join Claude stdin writer: {err}"))??;
    let status = child.wait().await?;
    let stderr = stderr_task
        .await
        .map_err(|err| format!("failed to join Claude stderr reader: {err}"))??;

    if !status.success() {
        return Err(format!(
            "Claude headless exited with {status}; stderr: {}",
            redacted_stderr_tail(&stderr)
        )
        .into());
    }

    let text = parser.finish()?;
    Ok(AcpCompletionResponse { text, usage: None })
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

#[derive(Default)]
struct ClaudeStreamState {
    text: String,
    result_text: Option<String>,
}

impl ClaudeStreamState {
    fn handle_line<F>(&mut self, line: &str, on_delta: &mut F) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|err| format!("malformed Claude stream JSON: {err}: {trimmed}"))?;
        if contains_tool_event(&value) {
            return Err("Claude headless emitted a tool event in synthesis-only mode".into());
        }
        if let Some(delta) = value.pointer("/delta/text").and_then(Value::as_str) {
            self.push_delta(delta, on_delta)?;
            return Ok(());
        }
        if let Some(message_text) = assistant_message_text(&value) {
            if let Some(delta) = message_text.strip_prefix(&self.text) {
                self.push_delta(delta, on_delta)?;
            } else if self.text.is_empty() {
                self.push_delta(&message_text, on_delta)?;
            }
            return Ok(());
        }
        if let Some(result) = value.get("result").and_then(Value::as_str)
            && !result.trim().is_empty()
        {
            self.result_text = Some(result.to_string());
        }
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
        if !self.text.trim().is_empty() {
            return Ok(self.text);
        }
        if let Some(result) = self.result_text
            && !result.trim().is_empty()
        {
            return Ok(result);
        }
        Err("Claude headless returned no answer text".into())
    }
}

fn assistant_message_text(value: &Value) -> Option<String> {
    let content = value.pointer("/message/content")?.as_array()?;
    let mut out = String::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("text")
            && let Some(text) = block.get("text").and_then(Value::as_str)
        {
            out.push_str(text);
        }
    }
    if out.is_empty() { None } else { Some(out) }
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
mod tests {
    use super::*;

    #[test]
    fn claude_headless_command_uses_no_tool_stream_json_posture() {
        let spec = build_command(&HeadlessCommandRequest::new(
            Some("sonnet".to_string()),
            Some("system".to_string()),
        ))
        .unwrap();
        assert_eq!(spec.program, "claude");
        assert_eq!(spec.prompt_transport, PromptTransport::Stdin);
        assert!(spec.args.windows(2).any(|w| w == ["--tools", ""]));
        assert!(
            spec.args
                .windows(2)
                .any(|w| w == ["--output-format", "stream-json"])
        );
        assert!(spec.args.contains(&"--verbose".to_string()));
        assert!(
            spec.args
                .windows(2)
                .any(|w| w == ["--mcp-config", "{\"mcpServers\":{}}"])
        );
        assert!(!spec.args.join(" ").contains("bypassPermissions"));
    }

    #[test]
    fn claude_headless_parser_streams_text_deltas() {
        let mut state = ClaudeStreamState::default();
        let mut out = String::new();
        state
            .handle_line(
                r#"{"type":"content_block_delta","delta":{"text":"hel"}}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        state
            .handle_line(
                r#"{"type":"content_block_delta","delta":{"text":"lo"}}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(out, "hello");
        assert_eq!(state.finish().unwrap(), "hello");
    }

    #[test]
    fn claude_headless_parser_handles_cumulative_messages() {
        let mut state = ClaudeStreamState::default();
        let mut out = String::new();
        state
            .handle_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        state
            .handle_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello world"}]}}"#,
                &mut |d| {
                    out.push_str(d);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(out, "hello world");
        assert_eq!(state.finish().unwrap(), "hello world");
    }

    #[test]
    fn claude_headless_parser_rejects_tool_events() {
        let mut state = ClaudeStreamState::default();
        let err = state
            .handle_line(
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#,
                &mut |_| Ok(()),
            )
            .expect_err("tool events must fail closed");
        assert!(err.to_string().contains("tool event"));
    }
}

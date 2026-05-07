use super::common::{
    HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, append_bounded_tail,
    env_or_default, redacted_stderr_tail,
};
use super::env::apply_env_allowlist;
use crate::services::llm_backend::{CompletionRequest, CompletionResponse, LlmBackendConfig};
use serde_json::{Value, json};
use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

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
    let gemini_home = prepare_gemini_home(&req.backend)?;
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

    let timeout = completion_timeout(&req.backend);
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
        .env("XDG_CACHE_HOME", gemini_home.path().join(".cache"));

    command
        .spawn()
        .map_err(|err| format!("failed to spawn Gemini headless command: {err}").into())
}

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    complete_streaming(req, |_| Ok(())).await
}

fn joined_prompt(system_prompt: Option<&str>, user_prompt: &str) -> String {
    match system_prompt.map(str::trim).filter(|s| !s.is_empty()) {
        Some(system) => format!("{system}\n\n{user_prompt}"),
        None => user_prompt.to_string(),
    }
}

async fn kill_and_wait(child: &mut Child) -> String {
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

fn prepare_gemini_home(
    config: &LlmBackendConfig,
) -> Result<TempDir, Box<dyn StdError + Send + Sync>> {
    let temp = tempfile::Builder::new()
        .prefix("axon-gemini-headless-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated Gemini HOME: {err}"))?;
    let gemini_dir = temp.path().join(".gemini");
    fs::create_dir_all(&gemini_dir)?;
    fs::create_dir_all(temp.path().join(".config"))?;
    fs::create_dir_all(temp.path().join(".cache"))?;

    let source_home = gemini_source_home(config)?;
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

fn gemini_source_home(
    config: &LlmBackendConfig,
) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
    if let Some(path) = &config.gemini_home {
        return validate_source_home(path.clone());
    }
    let home = non_empty_env("HOME").map(PathBuf::from).ok_or_else(
        || -> Box<dyn StdError + Send + Sync> {
            "HOME is required to locate Gemini CLI auth files".into()
        },
    )?;
    validate_source_home(home)
}

fn validate_source_home(path: PathBuf) -> Result<PathBuf, Box<dyn StdError + Send + Sync>> {
    let metadata = fs::symlink_metadata(&path).map_err(|err| {
        format!(
            "failed to inspect Gemini source home {}: {err}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Gemini source home must not be a symlink: {}",
            path.display()
        )
        .into());
    }
    if !metadata.is_dir() {
        return Err(format!("Gemini source home must be a directory: {}", path.display()).into());
    }
    Ok(path)
}

fn non_empty_env(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn completion_timeout(config: &LlmBackendConfig) -> std::time::Duration {
    std::time::Duration::from_secs(config.completion_timeout_secs.max(1))
}

fn write_isolated_settings(path: &Path) -> Result<(), Box<dyn StdError + Send + Sync>> {
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

    fn handle_result(&mut self, value: &Value) -> Result<(), Box<dyn StdError + Send + Sync>> {
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

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn gemini_headless_timeout_returns_error_for_hung_child() {
        use crate::services::llm_backend::{CompletionRequest, LlmBackendConfig};
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let cmd = dir.path().join("fake-gemini");
        fs::write(&cmd, "#!/bin/sh\nsleep 5\n").unwrap();
        let mut perms = fs::metadata(&cmd).unwrap().permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&cmd, perms).unwrap();

        let mut req = CompletionRequest::new("hello");
        req.backend = LlmBackendConfig {
            gemini_cmd: cmd.display().to_string(),
            gemini_model: None,
            gemini_home: Some(dir.path().to_path_buf()),
            completion_concurrency: 1,
            completion_timeout_secs: 1,
            configured: true,
        };

        let err = complete_text(req)
            .await
            .expect_err("hung child should time out");
        assert!(err.to_string().contains("timed out"));
    }
}

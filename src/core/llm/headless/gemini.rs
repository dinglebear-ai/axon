mod home;
mod stream;

use super::common::{
    HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, env_or_default, joined_prompt,
    kill_and_wait, read_bounded_stderr, redacted_stderr_tail,
};
use super::env::apply_env_allowlist;
use crate::core::llm::{CompletionRequest, CompletionResponse, LlmBackendConfig};
use std::error::Error as StdError;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use stream::GeminiStreamState;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

const DEFAULT_GEMINI_MODEL: &str = "gemini-3.1-flash-lite-preview";
const PROMPT_ARG_MAX_BYTES: usize = 64 * 1024;
const STDIN_PROMPT_PREAMBLE: &str =
    "Read the complete task and context from stdin, then answer only that task.";

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
    let spec = HeadlessCommandSpec {
        prompt_transport: PromptTransport::Argument,
        ..spec
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
    let program = resolve_headless_program(&spec.program).unwrap_or_else(|_| spec.program.clone());
    if program.contains('/') || program.contains('\\') {
        let path = Path::new(&program);
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

    let prompt = joined_prompt(req.system_prompt.as_deref(), &req.user_prompt);
    let effective_transport = effective_prompt_transport(&spec, &prompt);
    let mut child = spawn_gemini_child(
        &spec,
        &gemini_home,
        cwd.path(),
        &prompt,
        effective_transport,
    )?;
    let stdin_task = if effective_transport == PromptTransport::Stdin {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("failed to open Gemini headless stdin")?;
        Some(tokio::spawn(async move {
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.shutdown().await
        }))
    } else {
        None
    };

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
            if let Some(stdin_task) = &stdin_task {
                stdin_task.abort();
            }
            stderr_task.abort();
            if let Some(stdin_task) = stdin_task {
                let _ = stdin_task.await;
            }
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
        if let Some(stdin_task) = stdin_task {
            let _ = stdin_task.await;
        }
        let _ = stderr_task.await;
        return Err(format!("{err}; cleanup: {cleanup}").into());
    }

    let stderr_task = await_stdin_writer(stdin_task, &mut child, stderr_task, timeout).await?;
    let status = wait_for_gemini_status(&mut child, &stderr_task, timeout).await?;
    let stderr = read_gemini_stderr(stderr_task, timeout).await?;

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

async fn await_stdin_writer(
    stdin_task: Option<JoinHandle<io::Result<()>>>,
    child: &mut Child,
    stderr_task: JoinHandle<io::Result<Vec<u8>>>,
    timeout: Duration,
) -> Result<JoinHandle<io::Result<Vec<u8>>>, Box<dyn StdError + Send + Sync>> {
    let Some(stdin_task) = stdin_task else {
        return Ok(stderr_task);
    };
    if let Err(err) = stdin_task
        .await
        .map_err(|err| format!("failed to join Gemini stdin writer: {err}"))?
    {
        let status_text = wait_status_text(child, timeout).await;
        let stderr_text = read_stderr_text(stderr_task, timeout).await;
        return Err(format!(
            "Gemini headless stdin write failed: {err}; process {status_text}; stderr: {stderr_text}"
        )
        .into());
    }
    Ok(stderr_task)
}

async fn wait_for_gemini_status(
    child: &mut Child,
    stderr_task: &JoinHandle<io::Result<Vec<u8>>>,
    timeout: Duration,
) -> Result<std::process::ExitStatus, Box<dyn StdError + Send + Sync>> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => Ok(status?),
        Err(_) => {
            let cleanup = kill_and_wait(child).await;
            stderr_task.abort();
            Err(format!(
                "Gemini headless timed out waiting for process exit after {} seconds; cleanup: {cleanup}",
                timeout.as_secs(),
            )
            .into())
        }
    }
}

async fn read_gemini_stderr(
    stderr_task: JoinHandle<io::Result<Vec<u8>>>,
    timeout: Duration,
) -> Result<Vec<u8>, Box<dyn StdError + Send + Sync>> {
    match tokio::time::timeout(timeout, stderr_task).await {
        Ok(joined) => joined
            .map_err(|err| format!("failed to join Gemini stderr reader: {err}"))?
            .map_err(Into::into),
        Err(_) => Err(format!(
            "Gemini headless timed out reading stderr after {} seconds",
            timeout.as_secs()
        )
        .into()),
    }
}

async fn wait_status_text(child: &mut Child, timeout: Duration) -> String {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status.to_string(),
        Ok(Err(wait_err)) => format!("wait failed: {wait_err}"),
        Err(_) => "timed out waiting for process exit".to_string(),
    }
}

async fn read_stderr_text(
    stderr_task: JoinHandle<io::Result<Vec<u8>>>,
    timeout: Duration,
) -> String {
    match tokio::time::timeout(timeout, stderr_task).await {
        Ok(Ok(Ok(stderr))) => redacted_stderr_tail(&stderr),
        Ok(Ok(Err(read_err))) => format!("stderr read failed: {read_err}"),
        Ok(Err(join_err)) => format!("stderr join failed: {join_err}"),
        Err(_) => "timed out reading stderr".to_string(),
    }
}

fn spawn_gemini_child(
    spec: &HeadlessCommandSpec,
    gemini_home: &TempDir,
    cwd: &Path,
    prompt: &str,
    effective_transport: PromptTransport,
) -> Result<Child, Box<dyn StdError + Send + Sync>> {
    let program = resolve_headless_program(&spec.program)?;
    let mut command = Command::new(&program);
    let mut args = spec.args.clone();
    if let Some(idx) = args.iter().position(|arg| arg == "--prompt")
        && let Some(value) = args.get_mut(idx + 1)
    {
        *value = match effective_transport {
            PromptTransport::Argument => prompt.to_string(),
            PromptTransport::Stdin => STDIN_PROMPT_PREAMBLE.to_string(),
        }
    }
    command
        .args(&args)
        .current_dir(cwd)
        .stdin(if effective_transport == PromptTransport::Stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
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

fn resolve_headless_program(program: &str) -> Result<String, Box<dyn StdError + Send + Sync>> {
    if program.contains('/') || program.contains('\\') || program != "gemini" {
        return Ok(program.to_string());
    }
    match std::process::Command::new("mise")
        .args(["which", "gemini"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                if let Ok(real_path) = fs::canonicalize(&path) {
                    return Ok(real_path.display().to_string());
                }
                return Ok(path);
            }
        }
        _ => {}
    }
    Ok(program.to_string())
}

fn effective_prompt_transport(spec: &HeadlessCommandSpec, prompt: &str) -> PromptTransport {
    if spec.prompt_transport == PromptTransport::Argument && prompt.len() <= PROMPT_ARG_MAX_BYTES {
        PromptTransport::Argument
    } else {
        PromptTransport::Stdin
    }
}

pub async fn complete_text(
    req: CompletionRequest,
) -> Result<CompletionResponse, Box<dyn StdError + Send + Sync>> {
    complete_streaming(req, |_| Ok(())).await
}

fn completion_timeout(config: &LlmBackendConfig) -> Duration {
    Duration::from_secs(config.completion_timeout_secs.max(1))
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

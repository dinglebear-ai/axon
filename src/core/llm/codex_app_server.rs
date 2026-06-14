//! `codex app-server` LLM backend.
//!
//! Spawns `codex app-server` over stdio per completion (mirroring the Gemini
//! headless backend), runs the JSON-RPC synthesis handshake, and streams the
//! assistant message back. Process config is isolated via a throwaway
//! `CODEX_HOME` (see [`home`]) so a synthesis call does not load the user's MCP
//! servers, hooks, or skills. Wire-protocol logic lives in [`protocol`].

mod home;
mod protocol;

use std::error::Error as StdError;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

use crate::core::llm::headless::common::{
    joined_prompt, kill_and_wait, read_bounded_stderr, redacted_stderr_tail,
};
use crate::core::llm::{CompletionRequest, CompletionResponse, LlmBackendConfig};
use protocol::{CodexStep, CodexStreamState};

type BoxError = Box<dyn StdError + Send + Sync>;

pub async fn complete_text(req: CompletionRequest) -> Result<CompletionResponse, BoxError> {
    complete_streaming(req, |_| Ok(())).await
}

/// Preflight check that the configured `codex` command is usable. Mirrors the
/// Gemini backend's `validate_config` for `ask` validation.
pub fn validate_config(config: &LlmBackendConfig) -> Result<(), BoxError> {
    validate_codex_cmd(config)
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    mut on_delta: F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    validate_codex_cmd(&req.backend)?;
    let home = home::prepare_codex_home(&req.backend)?;
    let cwd = tempfile::Builder::new()
        .prefix("axon-codex-cwd-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated codex cwd: {err}"))?;

    let mut child = spawn_codex_child(&req.backend, &home, cwd.path())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("failed to open codex app-server stdin")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("failed to open codex app-server stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("failed to open codex app-server stderr")?;
    let stderr_task = tokio::spawn(read_bounded_stderr(stderr));

    let prompt = joined_prompt(req.system_prompt.as_deref(), &req.user_prompt);
    let model = req
        .model
        .clone()
        .or_else(|| req.backend.codex_model.clone());
    let mut state = CodexStreamState::new(
        model,
        prompt,
        cwd.path().display().to_string(),
        env!("CARGO_PKG_VERSION"),
    );

    let timeout = req.backend.completion_timeout();
    let result = match tokio::time::timeout(
        timeout,
        run_handshake(&mut state, &mut stdin, stdout, &mut on_delta),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(format!("codex app-server timed out after {}s", timeout.as_secs()).into()),
    };

    // `codex app-server` is a persistent process — it does not exit after a
    // turn — so terminate it explicitly regardless of outcome.
    let cleanup = kill_and_wait(&mut child).await;
    let stderr_tail = collect_stderr(stderr_task).await;

    match result {
        // A completed turn can still fail `into_response` (no answer text). Carry
        // the already-collected stderr context into that error too, not just the
        // handshake-error path — it often explains an empty response.
        Ok(()) => state
            .into_response()
            .map_err(|err| format!("{err}{}", stderr_suffix(&stderr_tail)).into()),
        Err(err) => Err(format!("{err}; cleanup: {cleanup}{}", stderr_suffix(&stderr_tail)).into()),
    }
}

async fn run_handshake<F>(
    state: &mut CodexStreamState,
    stdin: &mut ChildStdin,
    stdout: ChildStdout,
    on_delta: &mut F,
) -> Result<(), BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    write_line(stdin, &state.initial_line()).await?;
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match state.handle_line(&line, on_delta)? {
                CodexStep::Continue => {}
                CodexStep::Send(messages) => {
                    for message in &messages {
                        write_line(stdin, message).await?;
                    }
                }
                CodexStep::Done => return Ok(()),
            },
            Ok(None) => {
                return Err("codex app-server stream ended before the turn completed".into());
            }
            Err(err) => return Err(Box::new(err) as BoxError),
        }
    }
}

fn spawn_codex_child(
    backend: &LlmBackendConfig,
    home: &TempDir,
    cwd: &Path,
) -> Result<Child, BoxError> {
    let mut command = Command::new(&backend.codex_cmd);
    command
        .arg("app-server")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    home::apply_codex_env_allowlist(&mut command);
    home::apply_codex_home_env(&mut command, home.path());
    command
        .spawn()
        .map_err(|err| format!("failed to spawn codex app-server: {err}").into())
}

fn validate_codex_cmd(backend: &LlmBackendConfig) -> Result<(), BoxError> {
    let program = backend.codex_cmd.trim();
    if program.is_empty() {
        return Err("AXON_CODEX_CMD must not be empty".into());
    }
    // Only validate explicit paths; bare command names resolve via PATH.
    if !(program.contains('/') || program.contains('\\')) {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(Path::new(program))
        .map_err(|err| format!("failed to inspect AXON_CODEX_CMD: {err}"))?;
    if metadata.file_type().is_symlink() {
        return Err("AXON_CODEX_CMD must not point to a symlink".into());
    }
    if !metadata.is_file() {
        return Err("AXON_CODEX_CMD must point to an executable file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("AXON_CODEX_CMD is not executable".into());
        }
    }
    Ok(())
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> Result<(), BoxError> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

async fn collect_stderr(task: JoinHandle<Result<Vec<u8>, io::Error>>) -> Vec<u8> {
    let mut task = task;
    match tokio::time::timeout(Duration::from_millis(200), &mut task).await {
        Ok(joined) => joined.ok().and_then(Result::ok).unwrap_or_default(),
        Err(_) => {
            task.abort();
            Vec::new()
        }
    }
}

fn stderr_suffix(stderr: &[u8]) -> String {
    let tail = redacted_stderr_tail(stderr);
    let trimmed = tail.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("; stderr: {trimmed}")
    }
}

#[cfg(test)]
#[path = "codex_app_server_tests.rs"]
mod tests;

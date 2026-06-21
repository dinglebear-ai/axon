//! `codex app-server` LLM backend.
//!
//! Spawns `codex app-server` over stdio per completion (mirroring the Gemini
//! headless backend), runs the JSON-RPC synthesis handshake, and streams the
//! assistant message back. Process config is isolated via a throwaway
//! `CODEX_HOME` (see [`home`]) so a synthesis call does not load the user's MCP
//! servers, hooks, or skills. Wire-protocol logic lives in [`protocol`].

mod capabilities;
mod home;
mod protocol;

pub use capabilities::CodexCapabilities;

use std::error::Error as StdError;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

use crate::core::llm::headless::common::{read_bounded_stderr, redacted_stderr_tail};
use crate::core::llm::{CompletionRequest, CompletionResponse, LlmBackendConfig};
use crate::core::logging::{log_info, log_warn};
use protocol::{CodexStep, CodexStreamState};

type BoxError = Box<dyn StdError + Send + Sync>;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

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
    let cwd = tempfile::Builder::new()
        .prefix("axon-codex-cwd-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated codex cwd: {err}"))?;

    // `_home_guard` owns the throwaway CODEX_HOME for the isolated path and must
    // stay alive for the child's lifetime. In passthrough mode there is no
    // throwaway home — the child runs against the user's real CODEX_HOME + env.
    let (_home_guard, mut child) = if req.backend.codex_load_user_config {
        let child = spawn_codex_child_passthrough(&req.backend, cwd.path())?;
        (None, child)
    } else {
        let home = home::prepare_codex_home(&req.backend)?;
        let child = spawn_codex_child(&req.backend, &home, cwd.path())?;
        (Some(home), child)
    };
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

    let mut state = CodexStreamState::from_request(
        &req,
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
    let cleanup = cleanup_codex_child(&mut child).await;
    let stderr_tail = collect_stderr(stderr_task).await;

    match (result, cleanup) {
        // A completed turn can still fail `into_response` (no answer text). Carry
        // the already-collected stderr context into that error too, not just the
        // handshake-error path — it often explains an empty response.
        (Ok(()), Ok(_cleanup)) => state
            .into_response()
            .map_err(|err| format!("{err}{}", stderr_diagnostics_suffix(&stderr_tail)).into()),
        // The turn succeeded; a cleanup failure (leaked/zombie child) is an
        // operational concern, not a reason to throw away a usable answer. Log it
        // and still return the completion. Only fall back to an error if there is
        // no answer text to return.
        (Ok(()), Err(cleanup_err)) => match state.into_response() {
            Ok(response) => {
                log_warn(&format!(
                    "codex app-server turn succeeded but child cleanup failed: {cleanup_err}{}",
                    stderr_diagnostics_suffix(&stderr_tail)
                ));
                Ok(response)
            }
            Err(err) => Err(format!(
                "codex app-server completed but cleanup failed: {cleanup_err}; {err}{}",
                stderr_diagnostics_suffix(&stderr_tail)
            )
            .into()),
        },
        (Err(err), Ok(cleanup)) => Err(format!(
            "{err}; cleanup: {cleanup}{}",
            stderr_diagnostics_suffix(&stderr_tail)
        )
        .into()),
        (Err(err), Err(cleanup_err)) => Err(format!(
            "{err}; cleanup failed: {cleanup_err}{}",
            stderr_diagnostics_suffix(&stderr_tail)
        )
        .into()),
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
    configure_codex_child_isolation(&mut command);
    command
        .spawn()
        .map_err(|err| format!("failed to spawn codex app-server: {err}").into())
}

/// Spawn `codex app-server` against the user's real Codex config — inheriting the
/// full environment so MCP servers, skills, and hooks load. This deliberately
/// surrenders the isolation of [`spawn_codex_child`]; gated behind
/// `AXON_CODEX_LOAD_USER_CONFIG`. The process group is still set so cleanup can
/// SIGKILL the whole tree.
fn spawn_codex_child_passthrough(
    backend: &LlmBackendConfig,
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
    // Inherit the ambient environment (PATH, tokens, OPENAI_API_KEY, etc.) so the
    // user's MCP servers can authenticate. Only pin CODEX_HOME when explicitly
    // overridden; otherwise Codex resolves its own default home.
    match home::resolve_user_codex_home(backend)? {
        Some(home) => {
            command.env("CODEX_HOME", home);
        }
        None => log_info(
            "codex load-user-config: no CODEX_HOME resolved; relying on Codex's \
             default home and ambient-environment auth (e.g. OPENAI_API_KEY)",
        ),
    }
    configure_codex_child_isolation(&mut command);
    command
        .spawn()
        .map_err(|err| format!("failed to spawn codex app-server (load-user-config): {err}").into())
}

#[cfg(unix)]
fn configure_codex_child_isolation(command: &mut Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_codex_child_isolation(_command: &mut Command) {}

#[cfg(unix)]
async fn cleanup_codex_child(child: &mut Child) -> Result<String, String> {
    let kill_result = match child.id() {
        Some(pid) => kill_process_group(pid),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "codex child pid unavailable",
        )),
    };
    let wait_result = tokio::time::timeout(CLEANUP_TIMEOUT, child.wait()).await;
    match (kill_result, wait_result) {
        (Ok(()), Ok(Ok(status))) => Ok(format!("killed process group and reaped with {status}")),
        (Ok(()), Ok(Err(wait_err))) => {
            Err(format!("killed process group but wait failed: {wait_err}"))
        }
        (Ok(()), Err(_)) => Err(format!(
            "killed process group but wait timed out after {}s",
            CLEANUP_TIMEOUT.as_secs()
        )),
        (Err(kill_err), Ok(Ok(status))) => Err(format!(
            "process group kill failed: {kill_err}; wait returned {status}"
        )),
        (Err(kill_err), Ok(Err(wait_err))) => Err(format!(
            "process group kill failed: {kill_err}; wait failed: {wait_err}"
        )),
        (Err(kill_err), Err(_)) => Err(format!(
            "process group kill failed: {kill_err}; wait timed out after {}s",
            CLEANUP_TIMEOUT.as_secs()
        )),
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn kill_process_group(pid: u32) -> Result<(), io::Error> {
    // Send SIGKILL to the child's process group (negative pid) via syscall.
    // The child is its own group leader (`process_group(0)` at spawn), so this
    // reaps any app-server grandchildren too. Using libc directly avoids
    // depending on an external `kill` binary — slim container images
    // (config/Dockerfile) ship no procps, where shelling out fails with ENOENT.
    let pgid = i32::try_from(pid).map_err(|_| io::Error::other("codex child pid out of range"))?;
    // SAFETY: `kill(2)` with a negative pid + SIGKILL is a plain signal send; it
    // dereferences no memory and cannot trigger UB.
    let rc = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    if rc == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    // ESRCH = no process in the group: the child (and its tree) already exited,
    // which is exactly the cleanup goal — treat it as success. A persistent
    // app-server can race-exit between the handshake returning and this signal;
    // surfacing ESRCH here would otherwise convert a completed turn into a bogus
    // "cleanup failed" error. Genuine faults (EPERM/EINVAL) still propagate.
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(err)
}

#[cfg(not(unix))]
async fn cleanup_codex_child(child: &mut Child) -> Result<String, String> {
    child
        .kill()
        .await
        .map_err(|err| format!("failed to kill codex child: {err}"))?;
    match tokio::time::timeout(CLEANUP_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => Ok(format!("killed child and reaped with {status}")),
        Ok(Err(err)) => Err(format!("killed child but wait failed: {err}")),
        Err(_) => Err(format!(
            "killed child but wait timed out after {}s",
            CLEANUP_TIMEOUT.as_secs()
        )),
    }
}

/// Probe the Codex app-server for capability information.
///
/// Spawns the Codex child, performs the `initialize` handshake, then sends
/// `model/list` and `account/rateLimits/read` requests and parses the
/// responses. Used by `axon doctor` when the backend is `codex-app-server`.
///
/// The probe is bounded by a 15-second hard timeout and always returns a
/// [`CodexCapabilities`] value — individual method failures are represented as
/// `Err` variants inside the struct rather than propagated.
pub async fn probe_codex_capabilities(backend: &LlmBackendConfig) -> CodexCapabilities {
    use capabilities::run_capability_probe;
    use std::time::Duration;

    const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

    let result = tokio::time::timeout(PROBE_TIMEOUT, async {
        validate_codex_cmd(backend)?;
        let cwd = tempfile::Builder::new()
            .prefix("axon-codex-probe-cwd-")
            .tempdir()
            .map_err(|e| format!("failed to create probe cwd: {e}"))?;
        let home = home::prepare_codex_home(backend)?;
        let mut child = spawn_codex_child(backend, &home, cwd.path())?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("failed to open codex stdin for capability probe")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("failed to open codex stdout for capability probe")?;
        let caps = run_capability_probe(&mut stdin, stdout, env!("CARGO_PKG_VERSION")).await;
        drop(stdin);
        let _ = cleanup_codex_child(&mut child).await;
        Ok::<CodexCapabilities, BoxError>(caps)
    })
    .await;

    match result {
        Ok(Ok(caps)) => caps,
        Ok(Err(e)) => {
            let msg = format!("codex capability probe failed: {e}");
            CodexCapabilities {
                models: Err(msg.clone()),
                rate_limits: Err(msg),
            }
        }
        Err(_) => {
            let msg = "codex capability probe timed out after 15s".to_string();
            CodexCapabilities {
                models: Err(msg.clone()),
                rate_limits: Err(msg),
            }
        }
    }
}

fn validate_codex_cmd(backend: &LlmBackendConfig) -> Result<(), BoxError> {
    let program = backend.codex_cmd.trim();
    if program.is_empty() {
        return Err("AXON_CODEX_CMD must not be empty".into());
    }
    // Only validate explicit paths; bare command names resolve via PATH. The
    // production image installs `@openai/codex` (see config/Dockerfile), so a
    // bare `codex` resolves in-container too — no host-only restriction.
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

async fn collect_stderr(task: JoinHandle<Result<Vec<u8>, io::Error>>) -> Result<Vec<u8>, String> {
    let mut task = task;
    match tokio::time::timeout(Duration::from_millis(200), &mut task).await {
        Ok(Ok(Ok(stderr))) => Ok(stderr),
        Ok(Ok(Err(err))) => Err(format!("failed to read codex stderr: {err}")),
        Ok(Err(err)) => Err(format!("failed to join codex stderr reader: {err}")),
        Err(_) => {
            task.abort();
            Err("timed out collecting codex stderr after cleanup".to_string())
        }
    }
}

fn stderr_diagnostics_suffix(stderr: &Result<Vec<u8>, String>) -> String {
    match stderr {
        Ok(stderr) => stderr_suffix(stderr),
        Err(err) => format!("; stderr diagnostics unavailable: {err}"),
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

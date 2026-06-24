//! `codex app-server` LLM backend.
//!
//! Routes synthesis calls through a bounded pool of long-lived `codex
//! app-server` children (see [`pool`]).  Each child is spawned once, runs the
//! `initialize` → `thread/start` handshake, and is reused for many
//! `turn/start` cycles.  Pool size = `completion_concurrency`
//! (env: `AXON_CODEX_COMPLETION_CONCURRENCY`, now defaults to 4).
//!
//! Wire-protocol logic lives in [`protocol`]; CODEX_HOME isolation in [`home`].

mod capabilities;
mod home;
pub(super) mod pool;
mod protocol;

pub use capabilities::CodexCapabilities;

use std::error::Error as StdError;
use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::process::Child;
use tokio::task::JoinHandle;

use crate::llm::headless::common::{joined_prompt, read_bounded_stderr, redacted_stderr_tail};
use crate::llm::{CompletionRequest, CompletionResponse, LlmBackendConfig, ReasoningEffort};

type BoxError = Box<dyn StdError + Send + Sync>;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn complete_text(req: CompletionRequest) -> Result<CompletionResponse, BoxError> {
    complete_streaming(req, |_| Ok(())).await
}

/// Preflight check that the configured `codex` command is usable.
pub fn validate_config(config: &LlmBackendConfig) -> Result<(), BoxError> {
    validate_codex_cmd(config)
}

pub async fn complete_streaming<F>(
    req: CompletionRequest,
    on_delta: F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    validate_codex_cmd(&req.backend)?;
    complete_via_pool(req, on_delta).await
}

/// Route a completion through the process pool.
async fn complete_via_pool<F>(
    req: CompletionRequest,
    mut on_delta: F,
) -> Result<CompletionResponse, BoxError>
where
    F: FnMut(&str) -> Result<(), BoxError> + Send,
{
    let pool = pool::pool_for(&req.backend);
    let timeout = req.backend.completion_timeout();
    let mut slot = pool.checkout(timeout).await?;

    let prompt = joined_prompt(req.system_prompt.as_deref(), &req.user_prompt);
    let model = req.model.as_deref().or(req.backend.codex_model.as_deref());
    let effort = req.effort.map(ReasoningEffort::as_wire);

    let result = tokio::time::timeout(
        timeout,
        pool::run_turn(
            &mut slot,
            &prompt,
            model,
            effort,
            &req.backend,
            &mut on_delta,
        ),
    )
    .await;

    match result {
        Ok(Ok(response)) => {
            pool.checkin(slot).await;
            Ok(response)
        }
        Ok(Err(err)) => {
            pool::discard_slot(slot, &format!("turn error: {err}")).await;
            Err(err)
        }
        Err(_) => {
            pool::discard_slot(slot, "turn timeout").await;
            Err(format!("codex app-server timed out after {}s", timeout.as_secs()).into())
        }
    }
}

// ── Internal helpers (pub(super) so pool.rs can reuse them) ─────────────────

pub(super) fn configure_codex_child_isolation(command: &mut tokio::process::Command) {
    configure_impl(command);
}

#[cfg(unix)]
fn configure_impl(command: &mut tokio::process::Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_impl(_command: &mut tokio::process::Command) {}

#[cfg(unix)]
pub(super) async fn cleanup_codex_child(child: &mut Child) -> Result<String, String> {
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
        (Ok(()), Ok(Err(e))) => Err(format!("killed process group but wait failed: {e}")),
        (Ok(()), Err(_)) => Err(format!(
            "killed process group but wait timed out after {}s",
            CLEANUP_TIMEOUT.as_secs()
        )),
        (Err(kill_err), Ok(Ok(status))) => Err(format!(
            "process group kill failed: {kill_err}; wait returned {status}"
        )),
        (Err(kill_err), Ok(Err(e))) => Err(format!(
            "process group kill failed: {kill_err}; wait failed: {e}"
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
    let pgid = i32::try_from(pid).map_err(|_| io::Error::other("codex child pid out of range"))?;
    // SAFETY: `kill(2)` with a negative pid + SIGKILL is a plain signal send.
    let rc = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    if rc == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    // ESRCH = process already exited — treat as success.
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(err)
}

#[cfg(not(unix))]
pub(super) async fn cleanup_codex_child(child: &mut Child) -> Result<String, String> {
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
        let mut child = pool::spawn_child_isolated(backend, &home, cwd.path())?;
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

pub(super) fn read_bounded_stderr_spawn(
    stderr: tokio::process::ChildStderr,
) -> JoinHandle<Result<Vec<u8>, io::Error>> {
    tokio::spawn(read_bounded_stderr(stderr))
}

pub(super) async fn collect_stderr(
    task: JoinHandle<Result<Vec<u8>, io::Error>>,
) -> Result<Vec<u8>, String> {
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

pub(super) fn stderr_diagnostics_suffix(stderr: &Result<Vec<u8>, String>) -> String {
    match stderr {
        Ok(stderr) => stderr_suffix(stderr),
        Err(err) => format!("; stderr diagnostics unavailable: {err}"),
    }
}

fn validate_codex_cmd(backend: &LlmBackendConfig) -> Result<(), BoxError> {
    let program = backend.codex_cmd.trim();
    if program.is_empty() {
        return Err("AXON_CODEX_CMD must not be empty".into());
    }
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

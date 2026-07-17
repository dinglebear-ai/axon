//! Bounded async process execution for CLI tool `Execute` mode.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::CliToolError;
use crate::cli_tool::CliToolSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionOutcome {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: Option<i32>,
}

pub(crate) async fn execute_command(
    source: &CliToolSource,
) -> Result<ExecutionOutcome, CliToolError> {
    let mut command = Command::new(&source.command);
    command.args(&source.argv);
    command.env_clear();
    for key in &source.env_allowlist {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(|err| CliToolError {
        code: "tool.spawn_failed",
        message: format!("failed to spawn configured tool command: {err}"),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| pipe_failed("stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| pipe_failed("stderr"))?;
    let cap = source.output_cap_bytes;
    let stdout_reader = tokio::spawn(read_capped(stdout, cap));
    let stderr_reader = tokio::spawn(read_capped(stderr, cap));

    let timeout = Duration::from_millis(source.timeout_ms.max(1));
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(err)) => return Err(wait_failed(err)),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err(CliToolError {
                code: "tool.timeout",
                message: format!(
                    "configured tool command timed out after {}ms",
                    source.timeout_ms
                ),
            });
        }
    };

    let stdout = join_reader("stdout", stdout_reader).await?;
    let stderr = join_reader("stderr", stderr_reader).await?;
    Ok(ExecutionOutcome {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: status.code(),
    })
}

async fn join_reader(
    stream: &'static str,
    reader: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, CliToolError> {
    reader
        .await
        .map_err(|err| CliToolError {
            code: "tool.output_join_failed",
            message: format!("failed to join configured tool {stream} reader: {err}"),
        })?
        .map_err(|err| CliToolError {
            code: "tool.output_read_failed",
            message: format!("failed to read configured tool {stream}: {err}"),
        })
}

async fn read_capped<R>(mut pipe: R, cap: usize) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::with_capacity(cap.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = pipe.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = cap.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(captured)
}

fn pipe_failed(stream: &'static str) -> CliToolError {
    CliToolError {
        code: "tool.pipe_failed",
        message: format!("failed to capture configured tool {stream}"),
    }
}

fn wait_failed(err: std::io::Error) -> CliToolError {
    CliToolError {
        code: "tool.wait_failed",
        message: format!("failed to wait on configured tool command: {err}"),
    }
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;

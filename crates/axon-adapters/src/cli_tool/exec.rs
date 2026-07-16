//! Real process execution for the CLI tool adapter's `Execute` mode.
//!
//! No shell is ever involved: `std::process::Command::new(&source.command)`
//! with an explicit argv array (no `sh -c`, no string interpolation). The
//! child's environment is fully cleared and repopulated only from
//! `source.env_allowlist`. Output is capped per-stream, and the process is
//! killed if it runs past `source.timeout_ms`.

use std::io::Read;
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use super::CliToolError;
use crate::cli_tool::CliToolSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionOutcome {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: Option<i32>,
}

pub(crate) fn execute_command(source: &CliToolSource) -> Result<ExecutionOutcome, CliToolError> {
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
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|err| CliToolError {
        code: "tool.spawn_failed",
        message: format!("failed to spawn `{}`: {err}", source.command),
    })?;

    let cap = source.output_cap_bytes as u64;
    let stdout_handle = spawn_stdout_reader(child.stdout.take(), cap);
    let stderr_handle = spawn_stderr_reader(child.stderr.take(), cap);

    let timeout = Duration::from_millis(source.timeout_ms.max(1));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let status = child.wait().map_err(|err| wait_failed(source, err))?;
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    return Err(CliToolError {
                        code: "tool.timeout",
                        message: format!(
                            "command `{}` timed out after {}ms (exit status: {status})",
                            source.command, source.timeout_ms
                        ),
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(wait_failed(source, err)),
        }
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    Ok(ExecutionOutcome {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code: status.code(),
    })
}

fn wait_failed(source: &CliToolSource, err: std::io::Error) -> CliToolError {
    CliToolError {
        code: "tool.wait_failed",
        message: format!("failed to wait on `{}`: {err}", source.command),
    }
}

fn spawn_stdout_reader(pipe: Option<ChildStdout>, cap: u64) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || read_capped(pipe, cap))
}

fn spawn_stderr_reader(pipe: Option<ChildStderr>, cap: u64) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || read_capped(pipe, cap))
}

fn read_capped<R: Read>(pipe: Option<R>, cap: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    if let Some(pipe) = pipe {
        // Best-effort: a read error just yields whatever was captured so
        // far, rather than losing the whole document.
        let _ = pipe.take(cap).read_to_end(&mut buffer);
    }
    buffer
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;

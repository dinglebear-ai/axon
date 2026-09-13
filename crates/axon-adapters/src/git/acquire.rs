//! Git repository acquisition (clone) for `axon source <git-url>`.
//!
//! Ported from the legacy `axon-ingest::generic_git::clone_repo`: a shallow
//! (`--depth 1`) HTTPS clone into a throwaway [`tempfile::TempDir`]. The git
//! shared source pipeline then indexes the checked-out
//! tree, deriving provider/owner/repo identity from the original clone URL.
//!
//! Kept dependency-free of the removed legacy `axon-ingest` crate: argv
//! construction is a pure function so it can be asserted without
//! spawning `git`, and the clone honors `GIT_TERMINAL_PROMPT=0` so a private
//! repo fails fast instead of blocking on a credential prompt.

use std::ffi::OsStr;
use std::io;
use std::net::IpAddr;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axon_core::content::redact_url;
use axon_core::http::validate_resolved_ips;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use tokio::io::AsyncReadExt;

use crate::acquisition_security::validate_source_url;

/// Wall-clock cap for a single clone before it is aborted.
const CLONE_TIMEOUT: Duration = Duration::from_secs(300);

/// Classify whether `input` is a git repository target.
///
/// Thin wrapper over `git::parse_git_target` so transports
/// (CLI/MCP/web) can route on git-ness without depending on the adapter crate
/// directly or reimplementing URL parsing.
pub fn is_git_target(input: &str) -> bool {
    super::parse_git_target(input).is_ok()
}

/// Build the `git clone` argv for a shallow, no-prompt HTTPS clone.
///
/// Pure — spawns nothing — so callers can assert the exact command shape. The
/// `--` terminator guards against a clone URL that looks like a flag.
fn clone_argv(clone_url: &str, dest: &str, curl_resolve: &str) -> Vec<String> {
    vec![
        "-c".to_string(),
        format!("http.curloptResolve={curl_resolve}"),
        "-c".to_string(),
        "http.followRedirects=false".to_string(),
        "clone".to_string(),
        "--depth=1".to_string(),
        "--no-tags".to_string(),
        "--".to_string(),
        clone_url.to_string(),
        dest.to_string(),
    ]
}

/// Shallow-clone `clone_url` into a fresh temp directory.
///
/// The URL is SSRF-validated before spawning `git`. On success the returned
/// [`tempfile::TempDir`] owns the checkout; drop it to clean up. On failure the
/// clone stderr is URL-redacted before being surfaced.
pub async fn clone_git_repo(clone_url: &str) -> Result<tempfile::TempDir> {
    clone_git_repo_with_program(clone_url, OsStr::new("git")).await
}

async fn clone_git_repo_with_program(
    clone_url: &str,
    git_program: &OsStr,
) -> Result<tempfile::TempDir> {
    clone_git_repo_with_program_and_timeout(clone_url, git_program, CLONE_TIMEOUT).await
}

async fn clone_git_repo_with_program_and_timeout(
    clone_url: &str,
    git_program: &OsStr,
    timeout: Duration,
) -> Result<tempfile::TempDir> {
    validate_source_url(clone_url)
        .await
        .map_err(|err| anyhow::anyhow!("refusing to clone {}: {err}", redact_url(clone_url)))?;

    let curl_resolve = resolve_git_transport(clone_url).await?;
    let tmp = tempfile::tempdir().context("failed to create temp dir for git clone")?;
    let dest = tmp.path().to_string_lossy().to_string();
    let argv = clone_argv(clone_url, &dest, &curl_resolve);

    let mut command = tokio::process::Command::new(git_program);
    command
        .args(&argv)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = command
        .group()
        .kill_on_drop(true)
        .spawn()
        .context("failed to spawn git clone")?;
    let mut owned = CloneProcess::new(child, tmp);
    let output = match tokio::time::timeout(timeout, wait_for_clone(&mut owned)).await {
        Ok(result) => result?,
        Err(_) => {
            owned.cleanup().await;
            bail!(
                "git clone timed out after {}s for {}",
                timeout.as_secs_f64(),
                redact_url(clone_url)
            );
        }
    };

    if output.status.success() {
        return Ok(owned.finish());
    }

    let stderr = redact_url(String::from_utf8_lossy(&output.stderr).trim());
    owned.cleanup().await;
    bail!("git clone failed for {}: {stderr}", redact_url(clone_url));
}

async fn wait_for_clone(owned: &mut CloneProcess) -> io::Result<std::process::Output> {
    let mut stdout = owned
        .child_mut()
        .inner()
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("git clone stdout was not piped"))?;
    let mut stderr = owned
        .child_mut()
        .inner()
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("git clone stderr was not piped"))?;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let (stdout_result, stderr_result) = tokio::join!(
        stdout.read_to_end(&mut stdout_bytes),
        stderr.read_to_end(&mut stderr_bytes)
    );
    stdout_result?;
    stderr_result?;
    // Keep the leader unreaped while descendants can still own its pipes. This
    // prevents its process-group identifier from being recycled before a
    // cancellation or timeout can terminate the complete group.
    let status = owned.child_mut().wait().await?;
    Ok(std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

struct CloneProcess {
    child: Option<AsyncGroupChild>,
    checkout: Option<tempfile::TempDir>,
}

impl CloneProcess {
    fn new(child: AsyncGroupChild, checkout: tempfile::TempDir) -> Self {
        Self {
            child: Some(child),
            checkout: Some(checkout),
        }
    }

    fn child_mut(&mut self) -> &mut AsyncGroupChild {
        self.child.as_mut().expect("clone child must be present")
    }

    async fn cleanup(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        self.checkout.take();
    }

    fn finish(mut self) -> tempfile::TempDir {
        self.child.take();
        self.checkout.take().expect("checkout must be present")
    }
}

impl Drop for CloneProcess {
    fn drop(&mut self) {
        let mut child = self.child.take();
        let checkout = self.checkout.take();
        if child.is_none() && checkout.is_none() {
            return;
        }
        // start_kill is synchronous: Unix sends to the process group and
        // Windows terminates the Job Object before checkout cleanup can race a
        // still-running git helper. The spawned task only performs reaping.
        if let Some(child) = child.as_mut() {
            let _ = child.start_kill();
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(mut child) = child {
                    let _ = child.wait().await;
                }
                drop(checkout);
            });
        } else {
            drop(child);
            drop(checkout);
        }
    }
}

async fn resolve_git_transport(clone_url: &str) -> Result<String> {
    let parsed = url::Url::parse(clone_url).context("invalid git clone URL")?;
    let host = parsed
        .host_str()
        .context("git clone URL is missing a host")?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let mut ips = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .with_context(|| format!("failed to resolve git host {host}"))?
            .map(|address| address.ip())
            .collect::<Vec<_>>()
    };
    ips.sort_unstable();
    ips.dedup();
    if ips.is_empty() {
        bail!("git host {host} resolved to no addresses");
    }
    validate_resolved_ips(host, ips.iter().copied())?;
    let addresses = ips
        .iter()
        .map(|ip| match ip {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("{host}:{port}:{addresses}"))
}

#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;

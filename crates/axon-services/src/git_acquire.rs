//! Git repository acquisition (clone) for `axon source <git-url>`.
//!
//! Ported from the legacy `axon-ingest::generic_git::clone_repo`: a shallow
//! (`--depth 1`) HTTPS clone into a throwaway [`tempfile::TempDir`]. The git
//! bridge ([`crate::index_git_source_with_job`]) then indexes the checked-out
//! tree, deriving provider/owner/repo identity from the original clone URL.
//!
//! Kept dependency-free of the legacy `axon-ingest` crate (which is slated for
//! removal): argv construction is a pure function so it can be asserted without
//! spawning `git`, and the clone honors `GIT_TERMINAL_PROMPT=0` so a private
//! repo fails fast instead of blocking on a credential prompt.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use axon_core::content::redact_url;
use axon_core::http::validate_url;

/// Wall-clock cap for a single clone before it is aborted.
const CLONE_TIMEOUT: Duration = Duration::from_secs(300);

/// Classify whether `input` is a git repository target.
///
/// Thin wrapper over [`axon_adapters::git::parse_git_target`] so transports
/// (CLI/MCP/web) can route on git-ness without depending on the adapter crate
/// directly or reimplementing URL parsing.
pub fn is_git_target(input: &str) -> bool {
    axon_adapters::git::parse_git_target(input).is_ok()
}

/// Build the `git clone` argv for a shallow, no-prompt HTTPS clone.
///
/// Pure — spawns nothing — so callers can assert the exact command shape. The
/// `--` terminator guards against a clone URL that looks like a flag.
fn clone_argv(clone_url: &str, dest: &str) -> Vec<String> {
    vec![
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
    validate_url(clone_url)
        .map_err(|err| anyhow::anyhow!("refusing to clone {}: {err}", redact_url(clone_url)))?;

    let tmp = tempfile::tempdir().context("failed to create temp dir for git clone")?;
    let dest = tmp.path().to_string_lossy().to_string();
    let argv = clone_argv(clone_url, &dest);

    let mut command = tokio::process::Command::new("git");
    command.args(&argv).env("GIT_TERMINAL_PROMPT", "0");

    let output = tokio::time::timeout(CLONE_TIMEOUT, command.output())
        .await
        .map_err(|_| anyhow::anyhow!("git clone timed out for {}", redact_url(clone_url)))?
        .context("failed to spawn git clone")?;

    if output.status.success() {
        return Ok(tmp);
    }

    let stderr = redact_url(String::from_utf8_lossy(&output.stderr).trim());
    bail!("git clone failed for {}: {stderr}", redact_url(clone_url));
}

#[cfg(test)]
#[path = "git_acquire_tests.rs"]
mod tests;

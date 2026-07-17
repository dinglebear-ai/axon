use super::super::common::internal_error;
use axon_core::paths::axon_data_base_dir;
use rmcp::ErrorData;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use uuid::Uuid;

pub const MCP_ARTIFACT_DIR_ENV: &str = "AXON_MCP_ARTIFACT_DIR";

/// Detect a context name from the client's working directory.
///
/// Walks up from CWD looking for a `.git` directory. If found, returns the
/// repo root's directory name. Otherwise returns the CWD's directory name.
/// Result is cached for the process lifetime (MCP server runs as a subprocess
/// whose CWD is fixed at launch).
pub fn client_context_name() -> &'static str {
    static CONTEXT: OnceLock<String> = OnceLock::new();
    CONTEXT.get_or_init(|| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        // Walk up looking for .git
        let mut dir = cwd.as_path();
        loop {
            if dir.join(".git").exists() {
                return dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "default".to_string());
            }
            match dir.parent() {
                Some(parent) if parent != dir => dir = parent,
                _ => break,
            }
        }
        // No git repo — use CWD dirname
        cwd.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "default".to_string())
    })
}

/// Return the artifact root directory.
///
/// Resolution order: `AXON_MCP_ARTIFACT_DIR` env var, then
/// `axon_data_base_dir()/artifacts` (`AXON_DATA_DIR` → `$HOME/.axon`).
/// The context subdirectory (from `client_context_name()`) is always appended.
///
/// Not cached with `OnceLock` because tests mutate env vars between runs
/// in the same process. The env reads are cheap relative to the disk I/O
/// that follows every call.
pub fn artifact_root() -> PathBuf {
    let base = std::env::var(MCP_ARTIFACT_DIR_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| axon_data_base_dir().join("artifacts"));
    base.join(client_context_name())
}

fn fallback_artifact_root() -> PathBuf {
    std::env::temp_dir()
        .join("axon-mcp")
        .join(client_context_name())
}

async fn ensure_dir(path: &Path) -> Result<(), std::io::Error> {
    // Use ensure_private_dir (0o700). MCP artifacts may include scraped
    // page content, search results, and ask answers — all derived from
    // user prompts that may contain sensitive context.
    axon_core::paths::ensure_private_dir_async(path.to_path_buf()).await
}

async fn is_writable(path: &Path) -> bool {
    let probe = path.join(format!(
        ".axon-write-probe-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    ));
    match tokio::fs::File::create(&probe).await {
        Ok(_) => {
            let _ = tokio::fs::remove_file(&probe).await;
            true
        }
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "mcp: artifact root write probe failed"
            );
            false
        }
    }
}

pub async fn ensure_artifact_root() -> Result<PathBuf, ErrorData> {
    let root = artifact_root();
    let primary_result = ensure_dir(&root).await;
    if primary_result.is_ok() && is_writable(&root).await {
        return Ok(root);
    }
    let fallback = fallback_artifact_root();
    if fallback != root {
        if let Err(e) = &primary_result {
            tracing::warn!(
                primary = %root.display(),
                error = %e,
                fallback = %fallback.display(),
                "mcp: primary artifact root unusable, falling back to /tmp"
            );
        } else {
            tracing::warn!(
                primary = %root.display(),
                fallback = %fallback.display(),
                "mcp: primary artifact root is not writable, falling back to /tmp"
            );
        }
        if let Err(fallback_err) = ensure_dir(&fallback).await {
            return Err(internal_error(format!(
                "artifact dir '{}' is not writable; fallback '{}' also failed ({fallback_err})",
                root.display(),
                fallback.display()
            )));
        }
        if !is_writable(&fallback).await {
            return Err(internal_error(format!(
                "artifact dir '{}' and fallback '{}' are both not writable",
                root.display(),
                fallback.display()
            )));
        }
        return Ok(fallback);
    }
    Err(internal_error(format!(
        "artifact dir '{}' is not writable",
        root.display()
    )))
}

/// Shared mutex for serializing tests that mutate `MCP_ARTIFACT_DIR_ENV` /
/// `AXON_DATA_DIR`.  Exported so sibling test modules in `artifacts/` can
/// coordinate without a separate lock.
#[cfg(test)]
pub(crate) static ARTIFACT_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;

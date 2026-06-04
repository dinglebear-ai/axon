use super::super::common::{internal_error, invalid_params};
use crate::core::paths::axon_data_base_dir;
use crate::services::types::ArtifactHandle;
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
    crate::core::paths::ensure_private_dir_async(path.to_path_buf()).await
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

pub async fn build_artifact_path(stem: &str, ext: &str) -> Result<PathBuf, ErrorData> {
    let root = ensure_artifact_root().await?;
    let (action, name) = split_artifact_stem(stem);
    Ok(root.join(action).join(format!("{name}.{ext}")))
}

fn split_artifact_stem(stem: &str) -> (String, String) {
    let mut parts = stem.splitn(2, '-');
    let action_raw = parts.next().unwrap_or("misc");
    let name_raw = parts.next().unwrap_or(stem);
    let action = sanitize_segment(action_raw, "misc");
    let name = sanitize_segment(name_raw, "artifact");
    (action, name)
}

fn sanitize_segment(raw: &str, fallback: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn reject_relative_traversal(candidate: &Path, label: &str) -> Result<(), ErrorData> {
    if candidate.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(invalid_params(format!(
            "{label} cannot contain traversal components"
        )));
    }
    Ok(())
}

pub async fn artifact_handle_for_path(
    kind: &str,
    path: &Path,
    bytes: u64,
    line_count: Option<u64>,
    job_id: Option<String>,
    url: Option<String>,
) -> Result<ArtifactHandle, ErrorData> {
    let root = tokio::fs::canonicalize(ensure_artifact_root().await?)
        .await
        .map_err(|e| internal_error(e.to_string()))?;
    let canonical = tokio::fs::canonicalize(path)
        .await
        .map_err(|e| invalid_params(format!("artifact path not found: {e}")))?;
    if !canonical.starts_with(&root) {
        return Err(invalid_params(format!(
            "artifact path must be inside {}",
            root.display()
        )));
    }
    ArtifactHandle::try_from_path(kind, &root, &canonical, bytes, line_count, job_id, url)
        .ok_or_else(|| invalid_params(format!("artifact path must be inside {}", root.display())))
}

pub async fn resolve_artifact_output_path(raw: &str) -> Result<PathBuf, ErrorData> {
    let candidate = PathBuf::from(raw);
    if candidate.as_os_str().is_empty() {
        return Err(invalid_params("output path cannot be empty"));
    }
    if candidate.is_absolute() {
        return Err(invalid_params(format!(
            "output path must be relative to {}",
            ensure_artifact_root().await?.display()
        )));
    }
    reject_relative_traversal(&candidate, "output path")?;
    let root = ensure_artifact_root().await?;
    let resolved = root.join(candidate);
    if let Some(parent) = resolved.parent() {
        let canonical_root = tokio::fs::canonicalize(&root)
            .await
            .map_err(|e| internal_error(e.to_string()))?;
        if tokio::fs::try_exists(parent)
            .await
            .map_err(|e| internal_error(e.to_string()))?
        {
            let canonical_parent = tokio::fs::canonicalize(parent)
                .await
                .map_err(|e| invalid_params(format!("output path parent invalid: {e}")))?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(invalid_params(format!(
                    "output path must stay inside {}",
                    canonical_root.display()
                )));
            }
        }
    }
    Ok(resolved)
}

/// Shared mutex for serializing tests that mutate `MCP_ARTIFACT_DIR_ENV` /
/// `AXON_DATA_DIR`.  Exported so sibling test modules in `artifacts/` can
/// coordinate without a separate lock.
#[cfg(test)]
pub(crate) static ARTIFACT_ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;

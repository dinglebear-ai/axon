use std::fmt;
use std::path::{Component, Path, PathBuf};

use axon_api::source::{AuthMode, AuthScope, AuthSnapshot, SourceKind};
use axon_core::http::validate_url;
use axon_error::{ApiError, ErrorStage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSecurityError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for SourceSecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SourceSecurityError {}

/// Enforce SSRF policy before HTTP fetch, Chrome render, artifact writes, jobs,
/// graph writes, or vector writes can be created for network sources.
pub fn enforce_network_source_policy(urls: &[&str]) -> Result<(), SourceSecurityError> {
    for url in urls {
        validate_url(url).map_err(|err| SourceSecurityError {
            code: "security.ssrf_denied",
            message: format!("network source denied before side effects: {err}"),
        })?;
    }
    Ok(())
}

/// Enforce local-source scope and high-risk path policy before filesystem reads.
pub fn enforce_local_source_policy(
    path: &str,
    has_local_scope: bool,
) -> Result<(), SourceSecurityError> {
    if !has_local_scope {
        return Err(SourceSecurityError {
            code: "auth.scope_required",
            message: "local source requires axon:local or trusted local context".to_string(),
        });
    }
    if is_secret_like_local_path(path) {
        return Err(SourceSecurityError {
            code: "security.local_secret_denied",
            message: "secret-like local path denied before side effects".to_string(),
        });
    }
    Ok(())
}

/// Resolve an absolute local source against the configured server allowlist.
///
/// Both the requested path and every configured root must exist and contain no
/// symlink component. The returned path is canonical, but callers must still
/// acquire through a descriptor-relative boundary before reading it; this
/// function is the admission check, not a safe replacement for `openat2`.
pub fn enforce_local_source_allowed_roots(
    path: &Path,
    allowed_roots: &[PathBuf],
) -> Result<PathBuf, SourceSecurityError> {
    if allowed_roots.is_empty() || !path.is_absolute() || has_parent_component(path) {
        return Err(local_root_denied());
    }
    reject_symlink_components(path)?;
    let resolved = std::fs::canonicalize(path).map_err(|_| local_root_denied())?;

    for allowed_root in allowed_roots {
        if !allowed_root.is_absolute() || has_parent_component(allowed_root) {
            continue;
        }
        reject_symlink_components(allowed_root)?;
        let Ok(resolved_root) = std::fs::canonicalize(allowed_root) else {
            continue;
        };
        if resolved == resolved_root || resolved.starts_with(&resolved_root) {
            return Ok(resolved);
        }
    }
    Err(local_root_denied())
}

pub fn redact_local_path_for_public_payload(path: &str) -> String {
    if path.starts_with('/') || path.starts_with("~/") {
        "[redacted-local-path]".to_string()
    } else {
        path.to_string()
    }
}

pub(crate) fn authorize_local_source_policy(
    input: &str,
    kind: SourceKind,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    if kind != SourceKind::Local {
        return Ok(());
    }
    let has_local_scope = auth_snapshot
        .map(|snapshot| super::authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    enforce_local_source_policy(input, has_local_scope).map_err(source_security_api_error)
}

pub(crate) fn authorize_local_source_allowed_roots(
    input: &str,
    kind: SourceKind,
    auth_snapshot: Option<&AuthSnapshot>,
    allowed_roots: &[PathBuf],
) -> Result<(), ApiError> {
    if kind != SourceKind::Local
        || auth_snapshot.is_some_and(|snapshot| snapshot.auth_mode == AuthMode::TrustedLocal)
    {
        return Ok(());
    }
    enforce_local_source_allowed_roots(Path::new(input), allowed_roots)
        .map(|_| ())
        .map_err(source_security_api_error)
}

fn source_security_api_error(err: SourceSecurityError) -> ApiError {
    ApiError::new(err.code, ErrorStage::Authorizing, err.message)
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}

fn reject_symlink_components(path: &Path) -> Result<(), SourceSecurityError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current).map_err(|_| local_root_denied())?;
        if metadata.file_type().is_symlink() {
            return Err(local_root_denied());
        }
    }
    Ok(())
}

fn local_root_denied() -> SourceSecurityError {
    SourceSecurityError {
        code: "security.local_root_denied",
        message: "local source is outside configured allowed roots".to_string(),
    }
}

fn is_secret_like_local_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == ".env"
        || lower.ends_with("/.env")
        || lower.contains("/.ssh/")
        || lower.contains("/.codex/")
        || lower.contains("/.gemini/")
        || lower.contains("browser-profile")
        || lower.contains("cloud")
}

#[cfg(test)]
#[path = "../source_security_tests.rs"]
mod source_security_tests;

use std::fmt;

use axon_api::source::{AuthScope, AuthSnapshot};
use axon_core::http::validate_url;
use axon_error::{ApiError, ErrorStage};

use super::classify::SourceInputKind;

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

pub fn redact_local_path_for_public_payload(path: &str) -> String {
    if path.starts_with('/') || path.starts_with("~/") {
        "[redacted-local-path]".to_string()
    } else {
        path.to_string()
    }
}

pub(crate) fn authorize_local_source_policy(
    input: &str,
    kind: SourceInputKind,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    if kind != SourceInputKind::Local {
        return Ok(());
    }
    let has_local_scope = auth_snapshot
        .map(|snapshot| super::authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    enforce_local_source_policy(input, has_local_scope).map_err(source_security_api_error)
}

fn source_security_api_error(err: SourceSecurityError) -> ApiError {
    ApiError::new(err.code, ErrorStage::Authorizing, err.message)
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

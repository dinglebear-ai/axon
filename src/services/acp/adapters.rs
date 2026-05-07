//! ACP adapter detection, model override, and validation helpers.

use crate::services::types::AcpAdapterCommand;
use std::error::Error;
use std::ffi::OsStr;
use std::path::Path;

use serde::Deserialize;

// ── Adapter kind detection ──────────────────────────────────────────────────

/// Identifies which ACP adapter family a command belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpAdapterKind {
    Codex,
    Gemini,
    Other,
}

impl AcpAdapterKind {
    pub fn detect(adapter: &AcpAdapterCommand) -> Self {
        if is_codex_adapter(adapter) {
            Self::Codex
        } else if is_gemini_adapter(adapter) {
            Self::Gemini
        } else {
            Self::Other
        }
    }
}

pub(super) fn is_codex_adapter(adapter: &AcpAdapterCommand) -> bool {
    Path::new(&adapter.program)
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.contains("codex"))
}

pub(super) fn is_gemini_adapter(adapter: &AcpAdapterCommand) -> bool {
    Path::new(&adapter.program)
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.contains("gemini"))
}

// ── Model normalization & validation ────────────────────────────────────────

pub(super) fn normalized_requested_model(model: Option<&str>) -> Option<String> {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "default")
        .map(ToString::to_string)
}

pub(super) fn validate_model_string(model: &str) -> Result<(), Box<dyn Error>> {
    if model.is_empty() {
        return Err("model string is empty".into());
    }
    // Allow only alphanumeric, hyphens, underscores, dots, forward slashes, colons, spaces
    if !model
        .chars()
        .all(|c| c.is_alphanumeric() || "-_./: ".contains(c))
    {
        return Err(format!("model string contains invalid characters: {}", model).into());
    }
    Ok(())
}

// ── Model override appenders (FIX M-4/PERF-8: take adapter by value) ────────

/// Append `--model` flag to a Codex adapter command if a valid model is requested.
///
/// Takes `adapter` by value to avoid unnecessary `.clone()` on the no-op path.
pub(super) fn append_codex_model_override(
    adapter: AcpAdapterCommand,
    requested_model: Option<&str>,
) -> Result<AcpAdapterCommand, Box<dyn Error>> {
    let Some(model) = normalized_requested_model(requested_model) else {
        return Ok(adapter);
    };
    if !is_codex_adapter(&adapter) {
        return Ok(adapter);
    }
    // Skip Gemini model names forwarded to a Codex adapter (stale model from agent switch).
    if model.to_ascii_lowercase().starts_with("gemini") {
        return Ok(adapter);
    }
    validate_model_string(&model)?;
    let mut next = adapter;
    next.args.push("-c".to_string());
    // Safety: model is validated against the character allowlist in validate_model_string().
    // The quoted format model="<value>" is safe because args go via execvp (no shell
    // expansion). This assumption MUST remain true — do NOT use this format in shell
    // contexts.
    next.args.push(format!("model=\"{model}\""));
    Ok(next)
}

/// Append `--model` flag to a Gemini adapter command if a valid model is requested.
///
/// Takes `adapter` by value to avoid unnecessary `.clone()` on the no-op path.
pub(super) fn append_gemini_model_override(
    adapter: AcpAdapterCommand,
    requested_model: Option<&str>,
) -> Result<AcpAdapterCommand, Box<dyn Error>> {
    let Some(model) = normalized_requested_model(requested_model) else {
        return Ok(adapter);
    };
    if !is_gemini_adapter(&adapter) {
        return Ok(adapter);
    }
    // Only forward model names that look like Gemini models.
    // Claude/Codex model names (e.g. "sonnet", "opus", "o3") must not be
    // forwarded to the Gemini adapter -- the Google API returns
    // "Requested entity was not found." for unknown model identifiers.
    if !model.to_ascii_lowercase().starts_with("gemini") {
        return Ok(adapter);
    }
    validate_model_string(&model)?;
    let mut next = adapter;
    next.args.push("--model".to_string());
    next.args.push(model);
    Ok(next)
}

// ── Codex models cache (for config option fallback) ─────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct CodexModelsCache {
    pub models: Vec<CodexCachedModel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexCachedModel {
    pub slug: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
}

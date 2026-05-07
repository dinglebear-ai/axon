//! ACP adapter configuration directory discovery and model reading.

use crate::services::types::{AcpConfigOption, AcpConfigSelectValue};
use std::path::PathBuf;

use super::adapters::{CodexCachedModel, CodexModelsCache, normalized_requested_model};

// ── Config directory discovery ──────────────────────────────────────────────

pub(super) fn codex_config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty()) // ignore HOME="" — would resolve config relative to cwd
        .map(PathBuf::from)
        .map(|home| home.join(".codex"))
}

pub(super) fn gemini_config_dir() -> Option<PathBuf> {
    std::env::var_os("GEMINI_CLI_HOME")
        .filter(|v| !v.is_empty()) // ignore empty GEMINI_CLI_HOME — same cwd hazard
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".gemini"))
        })
}

// ── Default model readers ───────────────────────────────────────────────────

/// Read the default model from a Codex `config.toml`.
///
/// FIX L-5: Uses the `toml` crate for proper TOML parsing instead of naive
/// line-by-line scanning that could match commented-out or nested keys.
pub(super) async fn read_codex_default_model() -> Option<String> {
    let config_dir = codex_config_dir()?;
    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        return None;
    }
    let content = match tokio::fs::read_to_string(&config_path).await {
        Ok(c) => c,
        Err(err) => {
            crate::core::logging::log_warn(&format!(
                "ACP config: failed to read Codex config at {}: {err}",
                config_path.display()
            ));
            return None;
        }
    };
    let value: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(err) => {
            crate::core::logging::log_warn(&format!(
                "ACP config: malformed Codex TOML at {}: {err}",
                config_path.display()
            ));
            return None;
        }
    };
    value
        .get("model")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}

pub(super) async fn read_gemini_default_model() -> Option<String> {
    let config_dir = gemini_config_dir()?;
    let config_path = config_dir.join("settings.json");
    if !config_path.exists() {
        return None;
    }
    let raw = match tokio::fs::read_to_string(&config_path).await {
        Ok(r) => r,
        Err(err) => {
            crate::core::logging::log_warn(&format!(
                "ACP config: failed to read Gemini config at {}: {err}",
                config_path.display()
            ));
            return None;
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(err) => {
            crate::core::logging::log_warn(&format!(
                "ACP config: malformed Gemini JSON at {}: {err}",
                config_path.display()
            ));
            return None;
        }
    };
    parsed
        .pointer("/model/name")
        .or_else(|| parsed.get("selectedModel"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

// ── Cached model option builders ────────────────────────────────────────────

pub(super) async fn read_gemini_cached_model_options(
    current_model: Option<&str>,
) -> Option<Vec<AcpConfigOption>> {
    // Use if-let rather than `.or()` to avoid eagerly awaiting the fallback
    // when the normalized model is already known.
    let model = if let Some(m) = current_model.and_then(|v| normalized_requested_model(Some(v))) {
        m
    } else {
        read_gemini_default_model().await?
    };
    Some(vec![AcpConfigOption {
        id: "model".to_string(),
        name: "Model".to_string(),
        description: Some("Gemini model".to_string()),
        category: Some("model".to_string()),
        current_value: model.clone(),
        options: vec![AcpConfigSelectValue {
            value: model.clone(),
            name: model,
            description: None,
        }],
    }])
}

pub(super) async fn read_codex_cached_model_options(
    current_model: Option<&str>,
) -> Option<Vec<AcpConfigOption>> {
    let cache_path = codex_config_dir()?.join("models_cache.json");
    let raw = tokio::fs::read_to_string(cache_path).await.ok()?;
    let cache: CodexModelsCache = serde_json::from_str(&raw).ok()?;
    if cache.models.is_empty() {
        return None;
    }
    let options = cache
        .models
        .into_iter()
        .map(|model: CodexCachedModel| AcpConfigSelectValue {
            value: model.slug.clone(),
            name: model.display_name.unwrap_or_else(|| model.slug.clone()),
            description: model.description,
        })
        .collect::<Vec<_>>();
    // Resolve the preferred model slug from the current request or the persisted default.
    // Use if-let rather than `.or()` to avoid eagerly awaiting the fallback
    // when the normalized model is already known.
    let preferred = if let Some(m) = current_model.and_then(|v| normalized_requested_model(Some(v)))
    {
        Some(m)
    } else {
        read_codex_default_model().await
    };
    // Only use `preferred` as `current_value` when it's actually present in the
    // options list — a stale default from models_cache.json would produce an
    // unselectable value.  Fall back to the first available option.
    let selected = preferred
        .filter(|m| options.iter().any(|o| &o.value == m))
        .or_else(|| options.first().map(|o| o.value.clone()))?;
    Some(vec![AcpConfigOption {
        id: "model".to_string(),
        name: "Model".to_string(),
        description: Some("Codex cached model choices".to_string()),
        category: Some("model".to_string()),
        current_value: selected,
        options,
    }])
}

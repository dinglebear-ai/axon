//! Resolves the active saved provider profile into a per-field overlay.
//!
//! Selection precedence: `--provider` flag > `AXON_PROVIDER` env >
//! `[llm] active-provider` in config.toml. The resolved profile becomes a
//! [`ProviderOverlay`] that `config_literal` applies as `overlay.or(env).or(default)`
//! for the LLM backend fields — so an **active profile overrides the per-backend
//! `AXON_*` env vars** (an intentional exception to the global `env > toml` rule,
//! so that `provider use <name>` actually switches the backend). When no profile
//! is active the overlay is all-`None`, making resolution byte-identical to the
//! pre-profile behavior (fully backward compatible).

use super::super::toml_config::{TomlConfig, TomlProvider};
use crate::services::llm_backend::LlmBackendKind;

/// Per-field overlay sourced from the active provider profile. `None` fields
/// fall through to the env layer, so a profile may specify only what it needs
/// (e.g. an openai profile can omit `api-key` to inherit `AXON_OPENAI_API_KEY`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct ProviderOverlay {
    pub backend: Option<String>,
    pub gemini_model: Option<String>,
    pub gemini_cmd: Option<String>,
    pub gemini_home: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub openai_api_key: Option<String>,
    pub codex_model: Option<String>,
    pub codex_cmd: Option<String>,
    pub codex_home: Option<String>,
}

/// Resolve the active provider name and build its overlay. Returns the default
/// (empty) overlay when no provider is active. Errors when the active name has
/// no matching `[providers.<name>]` profile, or the profile's backend is
/// missing/invalid (fail fast — a typo'd backend should not surface only at
/// synthesis time).
pub(super) fn resolve_provider_overlay(
    toml: &TomlConfig,
    provider_flag: Option<&str>,
) -> Result<ProviderOverlay, String> {
    let Some(name) = active_provider_name(toml, provider_flag) else {
        return Ok(ProviderOverlay::default());
    };
    let profile = toml.providers.get(&name).ok_or_else(|| {
        format!(
            "active LLM provider '{name}' is not defined under [providers.{name}] in config.toml"
        )
    })?;
    overlay_from_profile(&name, profile)
}

/// The active provider profile name: `--provider` flag > `AXON_PROVIDER` env >
/// `[llm] active-provider`. Exposed to the parse layer so `provider list` and
/// config resolution agree on which profile is active.
pub(in crate::core::config::parse) fn active_provider_name(
    toml: &TomlConfig,
    provider_flag: Option<&str>,
) -> Option<String> {
    clean_str(provider_flag)
        .or_else(|| non_empty_env("AXON_PROVIDER"))
        .or_else(|| clean_str(toml.llm.active_provider.as_deref()))
}

/// The effective backend kind given an already-resolved overlay and the env
/// layer: active-profile backend > `AXON_LLM_BACKEND` > default. The single
/// source of truth for "which backend runs", shared by `config_literal`
/// (sets `cfg.llm_backend`) and `effective_backend_kind`.
pub(super) fn backend_from_overlay(overlay: &ProviderOverlay) -> Result<LlmBackendKind, String> {
    let raw = overlay
        .backend
        .clone()
        .or_else(|| non_empty_env("AXON_LLM_BACKEND"))
        .unwrap_or_default();
    LlmBackendKind::parse(&raw)
}

/// Resolve the effective backend straight from config + flag, resolving the
/// active provider overlay first. Errors when the active profile is missing or
/// has an invalid backend (the same fail-fast resolution a real `ask` uses) —
/// so `provider list` surfaces a broken active profile instead of a misleading
/// default. Exposed to the parse layer for the display path.
pub(in crate::core::config::parse) fn effective_backend_kind(
    toml: &TomlConfig,
    provider_flag: Option<&str>,
) -> Result<LlmBackendKind, String> {
    let overlay = resolve_provider_overlay(toml, provider_flag)?;
    backend_from_overlay(&overlay)
}

fn overlay_from_profile(name: &str, profile: &TomlProvider) -> Result<ProviderOverlay, String> {
    let backend = clean(&profile.backend)
        .ok_or_else(|| format!("provider '{name}' is missing a `backend` in [providers.{name}]"))?;
    let kind = LlmBackendKind::parse(&backend)
        .map_err(|err| format!("provider '{name}' has an invalid backend: {err}"))?;

    let mut overlay = ProviderOverlay {
        backend: Some(backend),
        ..ProviderOverlay::default()
    };
    let model = clean(&profile.model);
    let cmd = clean(&profile.cmd);
    let home = clean(&profile.home);
    match kind {
        LlmBackendKind::GeminiHeadless => {
            overlay.gemini_model = model;
            overlay.gemini_cmd = cmd;
            overlay.gemini_home = home;
        }
        LlmBackendKind::OpenAiCompat => {
            overlay.openai_model = model;
            overlay.openai_base_url = clean(&profile.base_url);
            overlay.openai_api_key = clean(&profile.api_key);
        }
        LlmBackendKind::CodexAppServer => {
            overlay.codex_model = model;
            overlay.codex_cmd = cmd;
            overlay.codex_home = home;
        }
    }
    Ok(overlay)
}

fn clean(value: &Option<String>) -> Option<String> {
    clean_str(value.as_deref())
}

fn clean_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
#[path = "provider_overlay_tests.rs"]
mod tests;

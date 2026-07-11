//! Config-hygiene diagnostics for `axon doctor` (env-contract.md Doctor Rules).
//!
//! Covers the four rules not already implemented by the service-reachability
//! and LLM-backend-auth probes:
//! - secrets present in `config.toml`
//! - tuning env vars that should move to TOML
//! - compose-only keys used outside compose
//! - deprecated env keys with target replacements
//!
//! Every check is read-only, best-effort (never panics/fails the doctor run),
//! and never echoes a secret *value* — only key/field names.

use crate::config::parse::env_registry::{EnvClassification, all_specs};
use crate::redact::{DefaultRedactor, RedactionContext, Redactor};
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConfigDiagnostic {
    pub check: &'static str,
    pub key: String,
    pub message: String,
    pub remediation: String,
}

/// Deprecated env keys that have a canonical replacement (as opposed to
/// keys the registry marks `Delete`, which have none). Both halves flow
/// through `.env`/env only. Deprecated pre-contract `config.toml` *section*
/// names (old `[llm]`/`[tei]`/`[scrape]`/`[services]`/...) are a separate,
/// stricter mechanism: `toml_config::parse_toml_config_str` hard-fails config
/// construction before `axon doctor` can even run, naming every offending
/// section and its 20-section-contract replacement in the error — so no
/// runtime doctor check is needed for them.
const DEPRECATED_ENV_REPLACEMENTS: &[(&str, &str)] = &[
    ("CHROME_URL", "AXON_CHROME_REMOTE_URL"),
    (
        "AXON_HEADLESS_GEMINI_MODEL",
        "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
    ),
    ("AXON_OPENAI_MODEL", "AXON_SYNTHESIS_OPENAI_MODEL"),
    ("AXON_CODEX_MODEL", "AXON_SYNTHESIS_CODEX_MODEL"),
];

/// Run all four config-hygiene checks and return their findings, most
/// actionable first (secrets, then tuning, then compose, then deprecated).
pub(super) fn run_all() -> Vec<ConfigDiagnostic> {
    let mut out = secrets_in_config_toml();
    out.extend(tuning_env_vars_present());
    out.extend(compose_only_env_vars_outside_compose());
    out.extend(deprecated_env_vars_present());
    out
}

fn resolve_config_toml_path() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("AXON_CONFIG_PATH") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    crate::paths::axon_config_path()
}

/// Scan `~/.axon/config.toml` (or `AXON_CONFIG_PATH`) for secret-shaped
/// field names/values using the shared redaction detector set. Never reads
/// or echoes the actual value — only the field path is reported.
fn secrets_in_config_toml() -> Vec<ConfigDiagnostic> {
    let Some(path) = resolve_config_toml_path() else {
        return Vec::new();
    };
    if !path.is_file() {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(toml_value) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    let Ok(json_value) = serde_json::to_value(&toml_value) else {
        return Vec::new();
    };

    let redactor = DefaultRedactor::new();
    let ctx = RedactionContext::cli_json();
    let (_, report) = redactor.redact_json(json_value, &ctx);

    report
        .redacted_fields
        .iter()
        .chain(report.dropped_fields.iter())
        .map(|field| ConfigDiagnostic {
            check: "secret_in_config_toml",
            key: field.clone(),
            message: format!("config.toml key `{field}` looks secret-shaped"),
            remediation: format!(
                "move the value for `{field}` out of config.toml into ~/.axon/.env (config.toml is meant to be safe to commit)"
            ),
        })
        .collect()
}

/// Env vars set in the process environment that the registry classifies as
/// `MoveToml` — a durable config.toml home exists for them, but env still
/// wins while both are present.
fn tuning_env_vars_present() -> Vec<ConfigDiagnostic> {
    all_specs()
        .filter(|spec| spec.classification == EnvClassification::MoveToml)
        .filter_map(|spec| env_non_empty(spec.key).map(|_| spec))
        .map(|spec| {
            let dest = spec.toml_destination.unwrap_or("config.toml");
            ConfigDiagnostic {
                check: "tuning_env_should_move_to_toml",
                key: spec.key.to_string(),
                message: format!(
                    "{} is a tuning knob set via environment; it has a config.toml home",
                    spec.key
                ),
                remediation: format!(
                    "move `{}` to `[{}]` in ~/.axon/config.toml (env still overrides while both are present)",
                    spec.key, dest
                ),
            }
        })
        .collect()
}

/// Compose-interpolation-only vars found set outside a Compose-managed
/// container. Best-effort heuristic: `AXON_IN_CONTAINER` marks the process
/// as running inside the axon container image, where Compose does read
/// these vars; its absence means the process is native/host, where these
/// vars are inert and their presence usually means copy-paste from a
/// compose `.env` into the host shell/`.env`.
fn compose_only_env_vars_outside_compose() -> Vec<ConfigDiagnostic> {
    if env_non_empty("AXON_IN_CONTAINER").is_some() {
        return Vec::new();
    }
    all_specs()
        .filter(|spec| spec.classification == EnvClassification::ComposeEnv)
        .filter_map(|spec| env_non_empty(spec.key).map(|_| spec))
        .map(|spec| ConfigDiagnostic {
            check: "compose_only_key_outside_compose",
            key: spec.key.to_string(),
            message: format!(
                "{} is a Docker Compose interpolation variable but this process is not running inside the axon container",
                spec.key
            ),
            remediation: format!(
                "remove `{}` from the host environment/.env unless you are intentionally overriding docker-compose.*.yaml interpolation",
                spec.key
            ),
        })
        .collect()
}

/// Deprecated env keys present in the environment: registry `Delete`
/// entries (stale, no replacement — remove) plus the small known
/// alias-with-replacement table above (legacy name still accepted, but a
/// canonical replacement exists).
fn deprecated_env_vars_present() -> Vec<ConfigDiagnostic> {
    let mut out: Vec<ConfigDiagnostic> = all_specs()
        .filter(|spec| spec.classification == EnvClassification::Delete)
        // Keys with a known replacement are reported once, below, with the
        // more specific "rename to X" remediation instead of "remove it".
        .filter(|spec| {
            !DEPRECATED_ENV_REPLACEMENTS
                .iter()
                .any(|(old, _)| *old == spec.key)
        })
        .filter_map(|spec| env_non_empty(spec.key).map(|_| spec))
        .map(|spec| ConfigDiagnostic {
            check: "deprecated_env_key",
            key: spec.key.to_string(),
            message: format!("{} is a stale env var with no replacement", spec.key),
            remediation: format!("remove `{}` from your environment", spec.key),
        })
        .collect();

    for (old, new) in DEPRECATED_ENV_REPLACEMENTS {
        if env_non_empty(old).is_some() {
            out.push(ConfigDiagnostic {
                check: "deprecated_env_key_with_replacement",
                key: (*old).to_string(),
                message: format!("{old} is deprecated; the canonical key is {new}"),
                remediation: format!("rename `{old}` to `{new}` in your environment/.env"),
            });
        }
    }
    out
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
#[path = "config_checks_tests.rs"]
mod tests;

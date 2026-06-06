//! Isolated `CODEX_HOME` preparation for headless synthesis.
//!
//! `codex app-server` loads `$CODEX_HOME/config.toml` on every spawn — MCP
//! servers, hooks, skills, plugins, OTLP exporters. Pointing it at the user's
//! real `~/.codex` makes a one-shot synthesis call spin up the entire MCP fleet,
//! run session hooks, and balloon the prompt to tens of thousands of tokens.
//!
//! Mirroring the Gemini headless backend, we build a throwaway `CODEX_HOME`:
//! copy only `auth.json` (so ChatGPT/API-key auth still works) and write a
//! minimal `config.toml` that disables MCP servers, the built-in apps tool
//! server, hooks, and OTLP. Auth via `OPENAI_API_KEY` in the environment also
//! works when no `auth.json` is present.

use crate::services::llm_backend::LlmBackendConfig;
use std::error::Error as StdError;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::process::Command;

type BoxError = Box<dyn StdError + Send + Sync>;

/// Environment variables forwarded to the `codex app-server` child. Everything
/// else is cleared; `CODEX_HOME` is set explicitly by the spawn path.
const ALLOWED_ENV_KEYS: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TZ",
    "TMPDIR",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// Clear the environment and forward only the allowlisted keys.
pub(super) fn apply_codex_env_allowlist(command: &mut Command) {
    command.env_clear();
    for key in ALLOWED_ENV_KEYS {
        if let Some(value) = std::env::var_os(key).filter(|value| !value.is_empty()) {
            command.env(key, value);
        }
    }
}

/// Build an isolated `CODEX_HOME` and return the owning temp dir.
pub(super) fn prepare_codex_home(config: &LlmBackendConfig) -> Result<TempDir, BoxError> {
    let temp = tempfile::Builder::new()
        .prefix("axon-codex-appserver-")
        .tempdir()
        .map_err(|err| format!("failed to create isolated CODEX_HOME: {err}"))?;

    if let Some(source) = codex_source_home(config)? {
        copy_auth(&source, temp.path())?;
    }
    write_isolated_config(temp.path(), config.codex_model.as_deref())?;
    Ok(temp)
}

fn copy_auth(source: &std::path::Path, dest: &std::path::Path) -> Result<(), BoxError> {
    let auth = source.join("auth.json");
    if auth.is_file() {
        fs::copy(&auth, dest.join("auth.json"))
            .map_err(|err| format!("failed to copy codex auth.json: {err}"))?;
    }
    Ok(())
}

/// Resolve the source `CODEX_HOME` to copy `auth.json` from: explicit config
/// override → `$CODEX_HOME` → `$HOME/.codex`. Returns `None` when no candidate
/// directory exists (env-based auth is still possible).
fn codex_source_home(config: &LlmBackendConfig) -> Result<Option<PathBuf>, BoxError> {
    if let Some(path) = &config.codex_home {
        return validate_source_home(path.clone()).map(Some);
    }
    if let Some(path) = non_empty_env("CODEX_HOME").map(PathBuf::from) {
        return Ok(existing_dir(path));
    }
    if let Some(home) = non_empty_env("HOME").map(PathBuf::from) {
        return Ok(existing_dir(home.join(".codex")));
    }
    Ok(None)
}

fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}

fn validate_source_home(path: PathBuf) -> Result<PathBuf, BoxError> {
    let metadata = fs::symlink_metadata(&path).map_err(|err| {
        format!(
            "failed to inspect AXON_CODEX_HOME {}: {err}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!("AXON_CODEX_HOME must not be a symlink: {}", path.display()).into());
    }
    if !metadata.is_dir() {
        return Err(format!("AXON_CODEX_HOME must be a directory: {}", path.display()).into());
    }
    Ok(path)
}

/// Write a minimal, side-effect-free `config.toml` into the isolated home.
fn write_isolated_config(dir: &std::path::Path, model: Option<&str>) -> Result<(), BoxError> {
    let mut toml = String::new();
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        // TOML basic string: escape backslashes and quotes.
        let escaped = model.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(toml, "model = \"{escaped}\"");
    }
    toml.push_str(
        "approval_policy = \"never\"\n\
         sandbox_mode = \"read-only\"\n\
         \n\
         [otel]\n\
         environment = \"off\"\n\
         \n\
         [features]\n\
         apps = false\n\
         hooks = false\n",
    );
    fs::write(dir.join("config.toml"), toml)
        .map_err(|err| format!("failed to write isolated codex config.toml: {err}"))?;
    Ok(())
}

fn non_empty_env(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "home_tests.rs"]
mod tests;

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
//!
//! This module serves both backend paths: the default isolated path
//! ([`prepare_codex_home`]) and the opt-in passthrough path
//! ([`resolve_user_codex_home`], used when `codex_load_user_config` is set),
//! which deliberately resolves the user's *real* `CODEX_HOME` so MCP servers,
//! skills, and hooks load.

use crate::core::llm::LlmBackendConfig;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::process::Command;

type BoxError = Box<dyn StdError + Send + Sync>;

/// Environment variables forwarded to the `codex app-server` child. Everything
/// else is cleared; `CODEX_HOME` is set explicitly by the spawn path.
const ALLOWED_ENV_KEYS: &[&str] = &[
    "PATH",
    "USER",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TZ",
    "TMPDIR",
    "OPENAI_API_KEY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

const MAX_CODEX_AUTH_JSON_BYTES: u64 = 1024 * 1024;

/// Clear the environment and forward only the allowlisted keys.
pub(super) fn apply_codex_env_allowlist(command: &mut Command) {
    apply_codex_env_allowlist_from(command, std::env::vars_os());
}

fn apply_codex_env_allowlist_from<I>(command: &mut Command, env: I)
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let env: BTreeMap<OsString, OsString> = env.into_iter().collect();
    command.env_clear();
    for key in ALLOWED_ENV_KEYS {
        if let Some(value) = env.get(OsStr::new(key)).filter(|value| !value.is_empty()) {
            command.env(key, value);
        }
    }
}

pub(super) fn apply_codex_home_env(command: &mut Command, home: &Path) {
    command.env("CODEX_HOME", home);
    command.env("HOME", home);
    command.env("XDG_CONFIG_HOME", home.join(".config"));
    command.env("XDG_CACHE_HOME", home.join(".cache"));
    command.env("XDG_DATA_HOME", home.join(".local/share"));
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

fn copy_auth(source: &Path, dest: &Path) -> Result<(), BoxError> {
    let auth = source.join("auth.json");
    let metadata = match fs::symlink_metadata(&auth) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("failed to inspect codex auth.json: {err}").into()),
    };
    if metadata.file_type().is_symlink() {
        return Err("codex auth.json must not be a symlink".into());
    }
    if !metadata.is_file() {
        return Err("codex auth.json must be a regular file".into());
    }
    if metadata.len() > MAX_CODEX_AUTH_JSON_BYTES {
        return Err("codex auth.json is larger than 1 MiB".into());
    }
    let dest_auth = dest.join("auth.json");
    fs::copy(&auth, &dest_auth).map_err(|err| format!("failed to copy codex auth.json: {err}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest_auth, fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("failed to chmod isolated codex auth.json: {err}"))?;
    }
    Ok(())
}

/// Resolve the user's real `CODEX_HOME` for the passthrough (load-user-config)
/// spawn path: explicit `AXON_CODEX_HOME` override → `$CODEX_HOME` → `$HOME/.codex`.
/// Returns `None` when no candidate exists (Codex falls back to its own default).
pub(super) fn resolve_user_codex_home(
    config: &LlmBackendConfig,
) -> Result<Option<PathBuf>, BoxError> {
    codex_source_home(config)
}

/// Resolve the source `CODEX_HOME` to copy `auth.json` from: explicit config
/// override → `$CODEX_HOME` → `$HOME/.codex`. Returns `None` when no candidate
/// directory exists (env-based auth is still possible).
fn codex_source_home(config: &LlmBackendConfig) -> Result<Option<PathBuf>, BoxError> {
    codex_source_home_from(config, non_empty_env("CODEX_HOME"), non_empty_env("HOME"))
}

/// Precedence core for [`codex_source_home`], with the `$CODEX_HOME` / `$HOME`
/// environment values injected so the resolution order is unit-testable without
/// mutating process-global env. Mirrors `apply_codex_env_allowlist_from`.
fn codex_source_home_from(
    config: &LlmBackendConfig,
    codex_home_env: Option<String>,
    home_env: Option<String>,
) -> Result<Option<PathBuf>, BoxError> {
    if let Some(path) = &config.codex_home {
        return validate_source_home(path.clone()).map(Some);
    }
    if let Some(path) = codex_home_env.map(PathBuf::from) {
        return existing_valid_source_home(path);
    }
    if let Some(home) = home_env.map(PathBuf::from) {
        return existing_valid_source_home(home.join(".codex"));
    }
    Ok(None)
}

#[cfg(test)]
fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}

fn existing_valid_source_home(path: PathBuf) -> Result<Option<PathBuf>, BoxError> {
    if !path.exists() {
        return Ok(None);
    }
    validate_source_home(path).map(Some)
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
fn write_isolated_config(dir: &Path, model: Option<&str>) -> Result<(), BoxError> {
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

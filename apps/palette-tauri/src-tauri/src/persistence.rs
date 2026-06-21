//! Disk persistence for the palette: reads/writes `settings.json`, the Axon
//! `.env`, and `config.toml`, plus the JSON↔TOML/env value conversions.
//!
//! # Allowlists
//!
//! `write_axon_env_values` and `write_axon_config_values` only write keys that
//! appear in `ALLOWED_ENV_KEYS` and `ALLOWED_TOML_SECTION_PREFIXES` (prefix
//! match) respectively.  Any key not on the list is rejected with a descriptive
//! error before touching disk.
//! This ensures renderer-supplied input cannot overwrite arbitrary keys.
//!
//! # Dotenv parser limitations
//!
//! The bundled dotenv parser/writer handles the common subset of dotenv syntax:
//! - `KEY=value`, `KEY="quoted"`, `KEY='single-quoted'`
//! - Blank lines and `#`-prefixed comment lines are preserved verbatim
//! - Inline comments (e.g. `KEY=val # comment`) are **not** stripped — the
//!   comment text becomes part of the value.  This is intentional: the writer
//!   only touches keys in the allowlist, so exotic line shapes that the palette
//!   never writes are preserved as-is.
//!
//! # Atomic writes
//!
//! Both `.env` and `config.toml` writes use an atomic rename pattern:
//! write to `<path>.tmp`, fsync, then `rename` to the target.  On Unix the
//! target file is created with mode `0o600`.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::{PaletteSettings, PartialPaletteSettings, SETTINGS_FILE};

/// Allowed keys for the `~/.axon/.env` layer.
///
/// Any key supplied by the renderer that is **not** in this set is rejected
/// before writing.  Add keys here when the palette gains the ability to
/// configure a new env variable.
const ALLOWED_ENV_KEYS: &[&str] = &[
    "AXON_DATA_DIR",
    "AXON_HOME",
    "QDRANT_URL",
    "TEI_URL",
    "AXON_CHROME_REMOTE_URL",
    "AXON_COLLECTION",
    "AXON_MCP_HTTP_HOST",
    "AXON_MCP_HTTP_PORT",
    "AXON_MCP_HTTP_TOKEN",
    "AXON_MCP_AUTH_MODE",
    "AXON_MCP_PUBLIC_URL",
    "AXON_MCP_GOOGLE_CLIENT_ID",
    "AXON_MCP_GOOGLE_CLIENT_SECRET",
    "AXON_MCP_AUTH_ADMIN_EMAIL",
    "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
    "AXON_MCP_ALLOWED_ORIGINS",
    "TAVILY_API_KEY",
    "AXON_SEARXNG_URL",
    "GITHUB_TOKEN",
    "GITLAB_TOKEN",
    "GITEA_TOKEN",
    "REDDIT_CLIENT_ID",
    "REDDIT_CLIENT_SECRET",
    "HF_TOKEN",
    "AXON_LLM_BACKEND",
    "AXON_OPENAI_BASE_URL",
    "AXON_OPENAI_MODEL",
    "AXON_OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "GEMINI_HOME",
    "AXON_HEADLESS_GEMINI_HOME",
    "AXON_HEADLESS_GEMINI_CMD",
    "AXON_HEADLESS_GEMINI_MODEL",
    "AXON_USER_AGENT",
    "AXON_CHROME_USER_AGENT",
    "AXON_LOG_PATH",
    "AXON_LOG_MAX_BYTES",
    "AXON_IMAGE",
    "AXON_MCP_HTTP_PUBLISH",
    "TEI_EMBEDDING_MODEL",
    "TEI_HTTP_PORT",
    "TEI_SERVER_MAX_CLIENT_BATCH_SIZE",
    "NVIDIA_VISIBLE_DEVICES",
    "CUDA_VISIBLE_DEVICES",
    // Connection fields managed directly by the palette settings UI
    "AXON_SERVER_URL",
];

/// Allowed dotted-key prefixes for the `~/.axon/config.toml` layer.
///
/// The palette uses dotted paths such as `search.collection`.  Any key
/// supplied by the renderer that does **not** start with one of these section
/// prefixes (or equal a section prefix exactly) is rejected.
const ALLOWED_TOML_SECTION_PREFIXES: &[&str] = &[
    "search.",
    "ask.",
    "tei.",
    "workers.",
    "chrome.",
    "scrape.",
    "verticals.",
    "antibot.",
    "payload.",
];

pub(crate) fn read_settings_result(app: &AppHandle) -> Result<PartialPaletteSettings, String> {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(err) => {
            crate::diag::warn(&err.to_string());
            return Ok(PartialPaletteSettings::default());
        }
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(PartialPaletteSettings::default());
        }
        Err(err) => {
            return Err(format!(
                "failed to read palette settings at {}: {err}",
                path.display()
            ));
        }
    };
    parse_settings_json(&contents, &path)
}

pub(crate) fn parse_settings_json(
    contents: &str,
    path: &Path,
) -> Result<PartialPaletteSettings, String> {
    serde_json::from_str(contents).map_err(|err| {
        format!(
            "failed to parse palette settings at {}: {err}",
            path.display()
        )
    })
}

pub(crate) fn write_settings(
    app: &AppHandle,
    settings: &PaletteSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut palette_only = settings.clone();
    palette_only.env_values.clear();
    palette_only.config_values.clear();
    atomic_write(
        &path,
        serde_json::to_string_pretty(&palette_only)?.as_bytes(),
    )?;
    Ok(())
}

pub(crate) fn settings_with_file_values(mut settings: PaletteSettings) -> PaletteSettings {
    settings.env_values = read_default_env_entries()
        .into_iter()
        .map(|(key, value)| (key, serde_json::Value::String(value)))
        .collect();
    settings.config_values = read_default_config_values();
    settings
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SETTINGS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

pub(crate) fn value_for(key: &str, file_entries: &[(String, String)]) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            file_entries
                .iter()
                .find(|(entry_key, _)| entry_key == key)
                .map(|(_, value)| value.clone())
        })
}

pub(crate) fn read_default_env_entries() -> Vec<(String, String)> {
    let Some(path) = default_env_path() else {
        return Vec::new();
    };
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            crate::diag::warn(&format!(
                "failed to read Axon env file at {}: {err}",
                path.display()
            ));
            return Vec::new();
        }
    };
    parse_env_entries(&contents)
}

fn default_env_path() -> Option<PathBuf> {
    std::env::var_os("AXON_ENV_PATH")
        .or_else(|| std::env::var_os("AXON_ENV_FILE"))
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".axon/.env")))
}

fn default_config_path() -> Option<PathBuf> {
    std::env::var_os("AXON_CONFIG_PATH")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".axon/config.toml")))
}

fn parse_env_entries(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), trim_env_value(value)))
        })
        .collect()
}

fn trim_env_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if bytes[0] == b'"' && bytes[value.len() - 1] == b'"' {
            return unescape_double_quoted(&value[1..value.len() - 1]);
        }
        if bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'' {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn unescape_double_quoted(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(crate) fn write_axon_env_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate all keys before touching disk.
    for key in values.keys() {
        if !ALLOWED_ENV_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "env key '{key}' is not in the palette allowlist; \
                 only recognised Axon env keys may be written"
            )
            .into());
        }
    }

    let path = default_env_path().ok_or("env path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(format!(
                "failed to read existing env file at {} before writing — \
                 refusing to continue to avoid data loss: {err}",
                path.display()
            )
            .into());
        }
    };
    let mut pending: HashMap<String, String> = values
        .iter()
        .map(|(key, value)| (key.clone(), json_value_to_env_string(value)))
        .collect();
    let mut lines = Vec::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }
        let Some((key, _)) = trimmed.split_once('=') else {
            lines.push(line.to_string());
            continue;
        };
        let key = key.trim();
        if let Some(value) = pending.remove(key) {
            lines.push(format!("{key}={}", format_env_value(&value)));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut remaining: Vec<_> = pending.into_iter().collect();
    remaining.sort_by(|(left, _), (right, _)| left.cmp(right));
    if !remaining.is_empty() && lines.last().is_some_and(|line| !line.is_empty()) {
        lines.push(String::new());
    }
    for (key, value) in remaining {
        lines.push(format!("{key}={}", format_env_value(&value)));
    }
    let mut output = lines.join("\n");
    output.push('\n');
    atomic_write(&path, output.as_bytes())?;
    Ok(())
}

pub(crate) fn read_default_config_values() -> HashMap<String, serde_json::Value> {
    let Some(path) = default_config_path() else {
        return HashMap::new();
    };
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return HashMap::new(),
        Err(err) => {
            crate::diag::warn(&format!(
                "failed to read Axon config file at {}: {err}",
                path.display()
            ));
            return HashMap::new();
        }
    };
    let doc = match contents.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(err) => {
            crate::diag::warn(&format!(
                "failed to parse Axon config file at {}: {err}",
                path.display()
            ));
            return HashMap::new();
        }
    };
    let mut values = HashMap::new();
    collect_toml_values("", doc.as_item(), &mut values);
    values
}

pub(crate) fn write_axon_config_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate all keys before touching disk.
    for key in values.keys() {
        let allowed = ALLOWED_TOML_SECTION_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix));
        if !allowed {
            return Err(format!(
                "config key '{key}' is not in the palette allowlist; \
                 only recognised Axon config.toml sections may be written"
            )
            .into());
        }
    }

    let path = default_config_path().ok_or("config path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(format!(
                "failed to read existing config file at {} before writing — \
                 refusing to continue to avoid data loss: {err}",
                path.display()
            )
            .into());
        }
    };
    let mut doc = match contents.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(err) => {
            return Err(format!(
                "failed to parse existing config file at {} — \
                 refusing to continue to avoid data loss: {err}",
                path.display()
            )
            .into());
        }
    };
    let mut entries: Vec<_> = values.iter().collect();
    entries.sort_by_key(|(key, _)| *key);
    for (path, value) in entries {
        set_toml_value(&mut doc, path, value);
    }
    atomic_write(&path, doc.to_string().as_bytes())?;
    Ok(())
}

/// Write `data` to `path` atomically: write to a per-write unique temp file,
/// then rename.
///
/// The temp name carries a UUID so two concurrent writers of the same `path`
/// (e.g. a login racing a refresh writing `oauth.json`) do not collide on a
/// fixed `<path>.tmp`.  If any step fails the temp file is best-effort removed
/// so unique temps don't accumulate on error.
///
/// On Unix, the temp file is created with mode `0o600` atomically via
/// `OpenOptions::mode`, so it is never world-readable even momentarily (no
/// umask window between `open` and a separate `chmod`).  On Windows no explicit
/// permission change is applied; rely on the directory ACL to restrict access.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }

            let mut file = opts.open(&tmp)?;

            use std::io::Write;
            file.write_all(data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn collect_toml_values(prefix: &str, item: &Item, values: &mut HashMap<String, serde_json::Value>) {
    match item {
        Item::Table(table) => {
            for (key, value) in table.iter() {
                let next = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_toml_values(&next, value, values);
            }
        }
        Item::Value(value) if !prefix.is_empty() => {
            values.insert(prefix.to_string(), toml_value_to_json(value));
        }
        _ => {}
    }
}

fn set_toml_value(doc: &mut DocumentMut, path: &str, value: &serde_json::Value) {
    let mut parts: Vec<&str> = path.split('.').collect();
    let Some(key) = parts.pop() else {
        return;
    };
    let mut current = doc.as_table_mut();
    for part in parts {
        if !current.contains_key(part) || !current[part].is_table() {
            current[part] = Item::Table(Table::new());
        }
        let Some(table) = current[part].as_table_mut() else {
            return;
        };
        current = table;
    }
    current[key] = json_value_to_toml_item(value);
}

fn toml_value_to_json(value: &toml_edit::Value) -> serde_json::Value {
    match value {
        toml_edit::Value::String(value) => serde_json::Value::String(value.value().to_string()),
        toml_edit::Value::Integer(value) => serde_json::Value::Number((*value.value()).into()),
        toml_edit::Value::Float(value) => serde_json::Number::from_f64(*value.value())
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml_edit::Value::Boolean(value) => serde_json::Value::Bool(*value.value()),
        toml_edit::Value::Array(array) => {
            serde_json::Value::Array(array.iter().map(toml_value_to_json).collect::<Vec<_>>())
        }
        _ => serde_json::Value::String(value.to_string()),
    }
}

fn json_value_to_toml_item(value: &serde_json::Value) -> Item {
    match value {
        serde_json::Value::Bool(value) => toml_edit::value(*value),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                toml_edit::value(value)
            } else if let Some(value) = value.as_f64() {
                toml_edit::value(value)
            } else {
                toml_edit::value(value.to_string())
            }
        }
        serde_json::Value::Array(values) => {
            let mut array = Array::default();
            for value in values {
                match value {
                    serde_json::Value::Bool(value) => {
                        array.push(*value);
                    }
                    serde_json::Value::Number(value) => {
                        if let Some(value) = value.as_i64() {
                            array.push(value);
                        } else if let Some(value) = value.as_f64() {
                            array.push(value);
                        }
                    }
                    _ => {
                        array.push(json_value_to_env_string(value));
                    }
                }
            }
            Item::Value(toml_edit::Value::Array(array))
        }
        _ => toml_edit::value(json_value_to_env_string(value)),
    }
}

fn json_value_to_env_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_value_to_env_string)
            .collect::<Vec<_>>()
            .join(","),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Object(_) => value.to_string(),
    }
}

fn format_env_value(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '#' | '$' | '\\'))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;

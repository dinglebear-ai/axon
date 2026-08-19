//! File-IO helpers for `axon config` — reads/writes ~/.axon/.env and
//! ~/.axon/config.toml without going through the full setup flow.

use crate::setup::config_store::{parse_env_pairs_from_str, render_env_value};
use axon_core::paths::{axon_config_path, axon_home_dir};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

/// Resolve the active .env path: `AXON_ENV_FILE` if set, else `~/.axon/.env`.
pub fn resolve_env_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("AXON_ENV_FILE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    axon_home_dir().map(|home| home.join(".env"))
}

/// Resolve the active config.toml path: `AXON_CONFIG_PATH` if set, else
/// `~/.axon/config.toml`.
pub fn resolve_toml_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("AXON_CONFIG_PATH") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    axon_config_path()
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct RemovedConfigKey {
    pub removed_key: &'static str,
    pub replacement: &'static str,
    pub target: &'static str,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ConfigRewriteEdit {
    pub path: String,
    pub removed_key: String,
    pub replacement: String,
    pub target: String,
    pub value_preview: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ConfigRewritePreview {
    pub dry_run: bool,
    pub env_path: Option<String>,
    pub toml_path: Option<String>,
    pub stale_keys: Vec<ConfigRewriteEdit>,
    pub write_count: usize,
    pub restart_required: bool,
}

pub const REMOVED_CONFIG_KEYS: &[RemovedConfigKey] = &[
    RemovedConfigKey {
        removed_key: "AXON_MCP_HTTP_HOST",
        replacement: "AXON_HTTP_HOST",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_HTTP_PORT",
        replacement: "AXON_HTTP_PORT",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_HTTP_TOKEN",
        replacement: "AXON_HTTP_TOKEN",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_AUTH_MODE",
        replacement: "AXON_AUTH_MODE",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_PUBLIC_URL",
        replacement: "AXON_PUBLIC_URL",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_GOOGLE_CLIENT_ID",
        replacement: "AXON_GOOGLE_CLIENT_ID",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_GOOGLE_CLIENT_SECRET",
        replacement: "AXON_GOOGLE_CLIENT_SECRET",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_AUTH_ADMIN_EMAIL",
        replacement: "AXON_AUTH_ADMIN_EMAIL",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        replacement: "AXON_ALLOWED_REDIRECT_URIS",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_MCP_ALLOWED_ORIGINS",
        replacement: "AXON_ALLOWED_ORIGINS",
        target: "env",
    },
    RemovedConfigKey {
        removed_key: "AXON_COLLECTION",
        replacement: "server.default-collection",
        target: "config.toml",
    },
    RemovedConfigKey {
        removed_key: "AXON_HYBRID_CANDIDATES",
        replacement: "retrieval.hybrid-candidates",
        target: "config.toml",
    },
    RemovedConfigKey {
        removed_key: "AXON_ASK_HYBRID_CANDIDATES",
        replacement: "ask.hybrid-candidates",
        target: "config.toml",
    },
    RemovedConfigKey {
        removed_key: "AXON_EMBED_DOC_TIMEOUT_SECS",
        replacement: "providers.embedding.doc-timeout-secs",
        target: "config.toml",
    },
    RemovedConfigKey {
        removed_key: "AXON_WATCH_TICK_SECS",
        replacement: "watch.tick-secs",
        target: "config.toml",
    },
    RemovedConfigKey {
        removed_key: "AXON_WATCH_LEASE_SECS",
        replacement: "watch.lease-secs",
        target: "config.toml",
    },
];

pub fn removed_config_key(key: &str) -> Option<&'static RemovedConfigKey> {
    REMOVED_CONFIG_KEYS
        .iter()
        .find(|entry| entry.removed_key == key)
}

pub fn config_rewrite_preview() -> io::Result<ConfigRewritePreview> {
    config_rewrite_preview_for_paths(resolve_env_path(), resolve_toml_path())
}

pub fn config_rewrite_apply() -> io::Result<ConfigRewritePreview> {
    config_rewrite_apply_for_paths(resolve_env_path(), resolve_toml_path())
}

pub fn config_rewrite_apply_for_paths(
    env_path: Option<PathBuf>,
    toml_path: Option<PathBuf>,
) -> io::Result<ConfigRewritePreview> {
    if let Some(path) = env_path.as_ref() {
        recover_config_rewrite(path)?;
    }
    let mut preview = config_rewrite_preview_for_paths(env_path.clone(), toml_path.clone())?;
    if preview.stale_keys.is_empty() {
        preview.dry_run = false;
        return Ok(preview);
    }

    let env_path = env_path
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "cannot resolve .env rewrite path"))?;
    let mut env_entries = read_env_entries(&env_path)?;
    let mut toml_document = match toml_path.as_ref() {
        Some(path) => read_toml_document(path)?,
        None => toml_edit::DocumentMut::new(),
    };
    let mut toml_changed = false;

    for edit in &preview.stale_keys {
        let value = env_entries
            .get(&edit.removed_key)
            .cloned()
            .ok_or_else(|| io::Error::other("stale config changed during rewrite"))?;
        match edit.target.as_str() {
            "env" => {
                if let Some(existing) = env_entries.get(&edit.replacement)
                    && existing != &value
                {
                    return Err(io::Error::new(
                        ErrorKind::AlreadyExists,
                        format!(
                            "refusing to overwrite {} while rewriting {}",
                            edit.replacement, edit.removed_key
                        ),
                    ));
                }
                env_entries.insert(edit.replacement.clone(), value);
            }
            "config.toml" => {
                if let Some(existing) = get_toml_entry(&toml_document, &edit.replacement) {
                    if existing != value.trim() {
                        return Err(io::Error::new(
                            ErrorKind::AlreadyExists,
                            format!(
                                "refusing to overwrite {} while rewriting {}",
                                edit.replacement, edit.removed_key
                            ),
                        ));
                    }
                } else {
                    set_toml_entry(&mut toml_document, &edit.replacement, &value)?;
                    toml_changed = true;
                }
            }
            target => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("unsupported rewrite target {target:?}"),
                ));
            }
        }
        env_entries.remove(&edit.removed_key);
    }

    let toml_write = if toml_changed {
        axon_core::config::parse::validate_toml_config_text(&toml_document.to_string())
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        let path = toml_path.as_deref().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "cannot resolve config.toml rewrite path",
            )
        })?;
        Some((path, &toml_document))
    } else {
        None
    };
    commit_config_rewrite(&env_path, &env_entries, toml_write, || Ok(()))?;

    preview.dry_run = false;
    preview.write_count = preview.stale_keys.len();
    Ok(preview)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ConfigRewriteJournal {
    env_original: Option<String>,
    toml: Option<ConfigRewriteFileJournal>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ConfigRewriteFileJournal {
    path: PathBuf,
    original: Option<String>,
}

fn rewrite_journal_path(env_path: &Path) -> PathBuf {
    let name = env_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".env");
    env_path.with_file_name(format!("{name}.rewrite-journal"))
}

fn read_optional_text(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn restore_optional_text(path: &Path, original: Option<&str>) -> io::Result<()> {
    match original {
        Some(contents) => write_private_file_atomic(path, contents),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
    }
}

fn recover_config_rewrite(env_path: &Path) -> io::Result<()> {
    let journal_path = rewrite_journal_path(env_path);
    let raw = match std::fs::read_to_string(&journal_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let journal: ConfigRewriteJournal = serde_json::from_str(&raw)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    restore_optional_text(env_path, journal.env_original.as_deref())?;
    if let Some(toml) = journal.toml.as_ref() {
        restore_optional_text(&toml.path, toml.original.as_deref())?;
    }
    std::fs::remove_file(journal_path)
}

fn commit_config_rewrite<F>(
    env_path: &Path,
    env_entries: &BTreeMap<String, String>,
    toml_write: Option<(&Path, &toml_edit::DocumentMut)>,
    after_toml_write: F,
) -> io::Result<()>
where
    F: FnOnce() -> io::Result<()>,
{
    let journal_path = rewrite_journal_path(env_path);
    let journal = ConfigRewriteJournal {
        env_original: read_optional_text(env_path)?,
        toml: toml_write
            .map(|(path, _)| -> io::Result<ConfigRewriteFileJournal> {
                Ok(ConfigRewriteFileJournal {
                    path: path.to_path_buf(),
                    original: read_optional_text(path)?,
                })
            })
            .transpose()?,
    };
    write_private_file_atomic(
        &journal_path,
        &serde_json::to_string(&journal).map_err(io::Error::other)?,
    )?;

    let write_result = (|| {
        if let Some((path, document)) = toml_write {
            write_toml_document(path, document)?;
        }
        after_toml_write()?;
        write_env_entries(env_path, env_entries)
    })();
    if let Err(error) = write_result {
        let restore_env = restore_optional_text(env_path, journal.env_original.as_deref());
        let restore_toml = journal
            .toml
            .as_ref()
            .map(|toml| restore_optional_text(&toml.path, toml.original.as_deref()))
            .transpose();
        if let Err(restore_error) = restore_env.and(restore_toml.map(|_| ())) {
            return Err(io::Error::other(format!(
                "{error}; rollback failed and {} was retained for recovery: {restore_error}",
                journal_path.display()
            )));
        }
        std::fs::remove_file(&journal_path)?;
        return Err(error);
    }
    std::fs::remove_file(journal_path)
}

pub fn config_rewrite_preview_for_paths(
    env_path: Option<PathBuf>,
    toml_path: Option<PathBuf>,
) -> io::Result<ConfigRewritePreview> {
    let env_entries = match env_path.as_ref() {
        Some(path) => read_env_entries(path)?,
        None => BTreeMap::new(),
    };
    let env_path_str = env_path.as_ref().map(|p| p.display().to_string());
    let toml_path_str = toml_path.as_ref().map(|p| p.display().to_string());
    let stale_keys = env_entries
        .iter()
        .filter_map(|(key, value)| {
            let removed = removed_config_key(key)?;
            Some(ConfigRewriteEdit {
                path: env_path_str.clone().unwrap_or_else(|| ".env".to_string()),
                removed_key: key.clone(),
                replacement: removed.replacement.to_string(),
                target: removed.target.to_string(),
                value_preview: display_env_value_for_preview(key, value),
            })
        })
        .collect::<Vec<_>>();
    Ok(ConfigRewritePreview {
        dry_run: true,
        env_path: env_path_str,
        toml_path: toml_path_str,
        restart_required: !stale_keys.is_empty(),
        stale_keys,
        write_count: 0,
    })
}

fn display_env_value_for_preview(key: &str, value: &str) -> String {
    let secret = key.contains("TOKEN") || key.contains("SECRET") || key.contains("KEY");
    if secret && !value.trim().is_empty() {
        "<redacted>".to_string()
    } else {
        value.to_string()
    }
}

pub fn read_env_entries(path: &Path) -> io::Result<BTreeMap<String, String>> {
    match std::fs::read_to_string(path) {
        Ok(raw) => parse_env_pairs_from_str(&raw),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(err),
    }
}

pub fn read_env_text(path: &Path) -> io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }
}

pub fn write_env_text(path: &Path, raw_env: &str) -> io::Result<()> {
    parse_env_pairs_from_str(raw_env)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    write_private_file_atomic(path, raw_env)
}

pub const PANEL_REDACTED_ENV_VALUE: &str = "[redacted-secret]";

/// Read the raw env editor surface without returning secret values to the
/// browser. Comments and non-sensitive assignments are preserved.
pub fn read_env_text_for_panel(path: &Path) -> io::Result<String> {
    let raw = read_env_text(path)?;
    Ok(redact_env_text_for_panel(&raw))
}

/// Save panel-edited env text while treating the redaction placeholder as
/// "keep the current secret". This lets the editor round-trip an existing
/// secret without ever receiving its value over REST.
pub fn write_env_text_from_panel(path: &Path, raw_env: &str) -> io::Result<()> {
    parse_env_pairs_from_str(raw_env)?;
    let existing = read_env_text(path)?;
    let merged = restore_redacted_panel_values(&existing, raw_env)?;
    write_env_text(path, &merged)
}

fn redact_env_text_for_panel(raw: &str) -> String {
    transform_env_lines(raw, |line| {
        let Some((key, _value)) = env_assignment(line) else {
            return Ok(line.to_string());
        };
        if axon_core::redact::is_secret_like(&key.to_ascii_lowercase()) {
            Ok(format!("{key}={PANEL_REDACTED_ENV_VALUE}"))
        } else {
            Ok(line.to_string())
        }
    })
    .expect("panel redaction is infallible")
}

fn restore_redacted_panel_values(existing: &str, submitted: &str) -> io::Result<String> {
    let existing_lines = existing
        .lines()
        .filter_map(|line| env_assignment(line).map(|(key, _)| (key.to_string(), line.to_string())))
        .collect::<BTreeMap<_, _>>();
    transform_env_lines(submitted, |line| {
        let Some((key, value)) = env_assignment(line) else {
            return Ok(line.to_string());
        };
        if axon_core::redact::is_secret_like(&key.to_ascii_lowercase())
            && value.trim() == PANEL_REDACTED_ENV_VALUE
        {
            return existing_lines.get(key).cloned().ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("{key} is redacted but has no existing value; enter a replacement"),
                )
            });
        }
        Ok(line.to_string())
    })
}

fn transform_env_lines<F>(raw: &str, mut transform: F) -> io::Result<String>
where
    F: FnMut(&str) -> io::Result<String>,
{
    let had_trailing_newline = raw.ends_with('\n');
    let mut lines = Vec::new();
    for line in raw.lines() {
        lines.push(transform(line.strip_suffix('\r').unwrap_or(line))?);
    }
    let mut output = lines.join("\n");
    if had_trailing_newline {
        output.push('\n');
    }
    Ok(output)
}

fn env_assignment(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    is_valid_env_key(key).then_some((key, value))
}

pub fn write_env_entries(path: &Path, env: &BTreeMap<String, String>) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::from("# Axon runtime env — managed by `axon config`.\n");
    for (key, value) in env {
        if value.contains(['\n', '\r']) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("{key} contains a newline and cannot be safely written"),
            ));
        }
        out.push_str(key);
        out.push('=');
        out.push_str(&render_env_value(value));
        out.push('\n');
    }
    write_private_file_atomic(path, &out)
}

pub fn set_env_entry(path: &Path, key: &str, value: &str) -> io::Result<()> {
    if !is_valid_env_key(key) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "{key:?} is not a valid env key (use UPPER_SNAKE, leading letter or underscore)"
            ),
        ));
    }
    let mut env = read_env_entries(path)?;
    env.insert(key.to_string(), value.to_string());
    write_env_entries(path, &env)
}

pub fn unset_env_entry(path: &Path, key: &str) -> io::Result<bool> {
    let mut env = read_env_entries(path)?;
    let removed = env.remove(key).is_some();
    if removed {
        write_env_entries(path, &env)?;
    }
    Ok(removed)
}

pub fn read_toml_document(path: &Path) -> io::Result<toml_edit::DocumentMut> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    raw.parse::<toml_edit::DocumentMut>()
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("TOML parse error: {err}")))
}

pub fn write_toml_document(path: &Path, document: &toml_edit::DocumentMut) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    write_private_file_atomic(path, &document.to_string())
}

/// Read the value at a dotted TOML path (e.g. `ask.cache.enabled`).
pub fn get_toml_entry(document: &toml_edit::DocumentMut, dotted: &str) -> Option<String> {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() {
        return None;
    }
    let mut current: &toml_edit::Item = document.as_item();
    for segment in &segments {
        match current.get(segment) {
            Some(item) => current = item,
            None => return None,
        }
    }
    Some(format_toml_item(current))
}

/// Set a value at a dotted TOML path. Intermediate tables are created as needed.
pub fn set_toml_entry(
    document: &mut toml_edit::DocumentMut,
    dotted: &str,
    raw_value: &str,
) -> io::Result<()> {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid TOML key path {dotted:?}"),
        ));
    }
    let last = segments[segments.len() - 1];
    let parents = &segments[..segments.len() - 1];
    let mut current: &mut toml_edit::Item = document.as_item_mut();
    for parent in parents {
        let next = current
            .as_table_like_mut()
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("{dotted:?}: expected table at segment {parent:?}"),
                )
            })?
            .entry(parent)
            .or_insert(toml_edit::table());
        current = next;
    }
    let table = current.as_table_like_mut().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("{dotted:?}: parent is not a table"),
        )
    })?;
    table.insert(last, parse_scalar(raw_value));
    Ok(())
}

/// Remove a dotted TOML key. Returns true if anything was removed.
pub fn unset_toml_entry(document: &mut toml_edit::DocumentMut, dotted: &str) -> io::Result<bool> {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid TOML key path {dotted:?}"),
        ));
    }
    let last = segments[segments.len() - 1];
    let parents = &segments[..segments.len() - 1];
    let removed = remove_toml_leaf(document.as_item_mut(), parents, last);
    if removed {
        prune_empty_toml_path(document.as_item_mut(), parents);
    }
    Ok(removed)
}

fn remove_toml_leaf(current: &mut toml_edit::Item, parents: &[&str], leaf: &str) -> bool {
    let Some((parent, rest)) = parents.split_first() else {
        return current
            .as_table_like_mut()
            .and_then(|table| table.remove(leaf))
            .is_some();
    };
    current
        .as_table_like_mut()
        .and_then(|table| table.get_mut(parent))
        .is_some_and(|next| remove_toml_leaf(next, rest, leaf))
}

/// Remove newly empty parent tables after a dotted key is unset. Without this,
/// unsetting the last key from a deprecated/renamed section leaves an empty
/// section that still fails the strict config-contract parser.
fn prune_empty_toml_path(current: &mut toml_edit::Item, parents: &[&str]) -> bool {
    let Some((parent, rest)) = parents.split_first() else {
        return current
            .as_table_like()
            .is_some_and(|table| table.is_empty());
    };
    let child_empty = current
        .as_table_like_mut()
        .and_then(|table| table.get_mut(parent))
        .is_some_and(|child| prune_empty_toml_path(child, rest));
    if child_empty && let Some(table) = current.as_table_like_mut() {
        table.remove(parent);
    }
    current
        .as_table_like()
        .is_some_and(|table| table.is_empty())
}

/// Flatten a TOML document into dotted key → string-value entries, walking only
/// scalars and scalar arrays.
pub fn flatten_toml(document: &toml_edit::DocumentMut) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk_table(document.as_table(), String::new(), &mut out);
    out
}

fn walk_table(table: &toml_edit::Table, prefix: String, out: &mut BTreeMap<String, String>) {
    for (key, item) in table.iter() {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        walk_item(item, path, out);
    }
}

fn walk_item(item: &toml_edit::Item, path: String, out: &mut BTreeMap<String, String>) {
    match item {
        toml_edit::Item::Value(v) => {
            out.insert(path, format_value(v));
        }
        toml_edit::Item::Table(t) => walk_table(t, path, out),
        toml_edit::Item::ArrayOfTables(arr) => {
            for (idx, sub) in arr.iter().enumerate() {
                walk_table(sub, format!("{path}[{idx}]"), out);
            }
        }
        toml_edit::Item::None => {}
    }
}

fn format_toml_item(item: &toml_edit::Item) -> String {
    match item {
        toml_edit::Item::Value(v) => format_value(v),
        other => other.to_string().trim().to_string(),
    }
}

fn format_value(value: &toml_edit::Value) -> String {
    match value {
        toml_edit::Value::String(s) => s.value().to_string(),
        toml_edit::Value::Boolean(b) => b.value().to_string(),
        toml_edit::Value::Integer(i) => i.value().to_string(),
        toml_edit::Value::Float(f) => f.value().to_string(),
        other => other.to_string().trim().to_string(),
    }
}

fn parse_scalar(raw: &str) -> toml_edit::Item {
    let trimmed = raw.trim();
    if let Ok(v) = trimmed.parse::<bool>() {
        return toml_edit::value(v);
    }
    if let Ok(v) = trimmed.parse::<i64>() {
        return toml_edit::value(v);
    }
    if let Ok(v) = trimmed.parse::<f64>()
        && trimmed.chars().any(|c| c == '.' || c == 'e' || c == 'E')
    {
        return toml_edit::value(v);
    }
    toml_edit::value(trimmed.to_string())
}

/// Returns true if the given env key is registered as a secret in the env
/// registry, or matches a fallback heuristic (`*_TOKEN`, `*_KEY`, `*_SECRET`,
/// `*_PASSWORD`).
pub fn is_secret_env_key(key: &str) -> bool {
    if let Some(spec) = axon_core::config::parse::env_registry::spec_for(key) {
        return spec.secret;
    }
    let upper = key.to_ascii_uppercase();
    upper.ends_with("_TOKEN")
        || upper.ends_with("_KEY")
        || upper.ends_with("_SECRET")
        || upper.ends_with("_PASSWORD")
        || upper == "TOKEN"
        || upper == "PASSWORD"
}

pub fn redact(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "***".to_string()
    }
}

// Env keys must be UPPER_SNAKE to match the auto-routing convention in
// `detect_target`: the router infers `.env` from uppercase keys and `.toml`
// from dotted lowercase paths. Accepting lowercase here would let callers
// write keys that auto-routing would silently misclassify on a later read.
fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_uppercase())
        && chars.all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn write_private_file_atomic(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "config path has no parent"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = parent.join(format!(
        ".{}.tmp.{stamp}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("axon-config")
    ));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);

    let mut file = options.open(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        // Persist the rename's directory-entry update so a crash between
        // rename and the next fsync can't lose the new file or revert to the
        // old contents.
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

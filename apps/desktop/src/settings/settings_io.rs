use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

pub(super) fn resolve_env_path() -> PathBuf {
    if let Some(path) = non_empty_env("AXON_ENV_FILE") {
        return PathBuf::from(path);
    }
    axon_home_dir().join(".env")
}

pub(super) fn resolve_toml_path() -> PathBuf {
    if let Some(path) = non_empty_env("AXON_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    axon_home_dir().join("config.toml")
}

pub(super) fn read_env_entries(path: &Path) -> io::Result<BTreeMap<String, String>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    Ok(parse_env_entries(&raw))
}

pub(super) fn write_env_updates(path: &Path, updates: &BTreeMap<String, String>) -> io::Result<()> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim();
            if let Some(value) = updates.get(key) {
                lines.push(format!("{key}={}", render_env_value(value)));
                seen.insert(key.to_string());
                continue;
            }
        }
        lines.push(line.to_string());
    }
    for (key, value) in updates {
        if seen.insert(key.clone()) {
            lines.push(format!("{key}={}", render_env_value(value)));
        }
    }
    write_private_file(path, &(lines.join("\n") + "\n"))
}

pub(super) fn read_toml_document(path: &Path) -> io::Result<toml_edit::DocumentMut> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    raw.parse::<toml_edit::DocumentMut>()
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("TOML parse error: {err}")))
}

pub(super) fn write_toml_updates(
    path: &Path,
    updates: &BTreeMap<String, String>,
) -> io::Result<()> {
    let mut doc = read_toml_document(path)?;
    for (key, value) in updates {
        set_toml_entry(&mut doc, key, value)?;
    }
    write_private_file(path, &doc.to_string())
}

pub(super) fn flatten_toml(doc: &toml_edit::DocumentMut) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    walk_table(doc.as_table(), String::new(), &mut out);
    out
}

fn axon_home_dir() -> PathBuf {
    if let Some(path) = non_empty_env("AXON_HOME") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".axon")
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_env_entries(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if is_valid_env_key(key) {
            out.insert(key.to_string(), unquote_env_value(value.trim()));
        }
    }
    out
}

fn set_toml_entry(
    doc: &mut toml_edit::DocumentMut,
    dotted: &str,
    raw_value: &str,
) -> io::Result<()> {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid TOML key path {dotted:?}"),
        ));
    }
    let mut current = doc.as_item_mut();
    for parent in &segments[..segments.len() - 1] {
        let table = current.as_table_like_mut().ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                format!("{dotted:?}: expected table at {parent:?}"),
            )
        })?;
        current = table.entry(parent).or_insert(toml_edit::table());
    }
    let table = current.as_table_like_mut().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            format!("{dotted:?}: parent is not a table"),
        )
    })?;
    table.insert(segments[segments.len() - 1], parse_toml_scalar(raw_value));
    Ok(())
}

fn walk_table(table: &toml_edit::Table, prefix: String, out: &mut BTreeMap<String, String>) {
    for (key, item) in table.iter() {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        match item {
            toml_edit::Item::Value(value) => {
                out.insert(path, format_toml_value(value));
            }
            toml_edit::Item::Table(table) => walk_table(table, path, out),
            _ => {}
        }
    }
}

fn parse_toml_scalar(raw: &str) -> toml_edit::Item {
    let trimmed = raw.trim();
    if let Ok(value) = trimmed.parse::<bool>() {
        toml_edit::value(value)
    } else if let Ok(value) = trimmed.parse::<i64>() {
        toml_edit::value(value)
    } else if let Ok(value) = trimmed.parse::<f64>() {
        toml_edit::value(value)
    } else if trimmed.starts_with('[') {
        trimmed
            .parse::<toml_edit::Value>()
            .map(toml_edit::Item::Value)
            .unwrap_or_else(|_| toml_edit::value(trimmed.to_string()))
    } else {
        toml_edit::value(trimmed.to_string())
    }
}

fn format_toml_value(value: &toml_edit::Value) -> String {
    match value {
        toml_edit::Value::String(value) => value.value().to_string(),
        toml_edit::Value::Boolean(value) => value.value().to_string(),
        toml_edit::Value::Integer(value) => value.value().to_string(),
        toml_edit::Value::Float(value) => value.value().to_string(),
        other => other.to_string().trim().to_string(),
    }
}

fn render_env_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':' | '@'))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())
}

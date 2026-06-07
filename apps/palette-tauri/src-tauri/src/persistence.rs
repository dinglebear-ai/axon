//! Disk persistence for the palette: reads/writes `settings.json`, the Axon
//! `.env`, and `config.toml`, plus the JSON↔TOML/env value conversions.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::{PaletteSettings, PartialPaletteSettings, SETTINGS_FILE};

pub(crate) fn read_settings_result(app: &AppHandle) -> Result<PartialPaletteSettings, String> {
    let Some(path) = settings_path(app) else {
        return Ok(PartialPaletteSettings::default());
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
    let path = settings_path(app).ok_or("settings path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut palette_only = settings.clone();
    palette_only.env_values.clear();
    palette_only.config_values.clear();
    fs::write(path, serde_json::to_string_pretty(&palette_only)?)?;
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

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(SETTINGS_FILE))
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
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
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
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

pub(crate) fn write_axon_env_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_env_path().ok_or("env path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
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
    if !remaining.is_empty() && !lines.last().is_none_or(|line| line.is_empty()) {
        lines.push(String::new());
    }
    for (key, value) in remaining {
        lines.push(format!("{key}={}", format_env_value(&value)));
    }
    let mut output = lines.join("\n");
    output.push('\n');
    fs::write(path, output)?;
    Ok(())
}

pub(crate) fn read_default_config_values() -> HashMap<String, serde_json::Value> {
    let Some(path) = default_config_path() else {
        return HashMap::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(doc) = contents.parse::<DocumentMut>() else {
        return HashMap::new();
    };
    let mut values = HashMap::new();
    collect_toml_values("", doc.as_item(), &mut values);
    values
}

pub(crate) fn write_axon_config_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_config_path().ok_or("config path unavailable")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = fs::read_to_string(&path).unwrap_or_default();
    let mut doc = contents
        .parse::<DocumentMut>()
        .unwrap_or_else(|_| DocumentMut::new());
    let mut entries: Vec<_> = values.iter().collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (path, value) in entries {
        set_toml_value(&mut doc, path, value);
    }
    fs::write(path, doc.to_string())?;
    Ok(())
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
        Item::Value(value) => {
            if !prefix.is_empty() {
                values.insert(prefix.to_string(), toml_value_to_json(value));
            }
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
        toml_edit::Value::Array(array) => serde_json::Value::Array(
            array
                .iter()
                .map(|item| toml_value_to_json(item))
                .collect::<Vec<_>>(),
        ),
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

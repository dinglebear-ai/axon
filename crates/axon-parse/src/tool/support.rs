use axon_api::source::{Severity, SourceWarning};
use serde_json::Value;

use crate::parser::ParseInput;

pub(super) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(super) fn redacted_paths(value: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    collect_redacted_paths(value, "", &mut paths);
    paths
}

fn collect_redacted_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::String(text) if is_redacted(text) => paths.push(if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        }),
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                collect_redacted_paths(item, &format!("{path}/{idx}"), paths);
            }
        }
        Value::Object(object) => {
            for (key, item) in object {
                collect_redacted_paths(item, &format!("{path}/{}", pointer_escape(key)), paths);
            }
        }
        _ => {}
    }
}

fn is_redacted(text: &str) -> bool {
    let normalized = text.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "[redacted]" | "<redacted>" | "redacted" | "*** redacted ***"
    ) || normalized.contains("[redacted]")
}

fn pointer_escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

pub(super) fn mutating_side_effect(side_effect_class: &str) -> bool {
    matches!(
        side_effect_class,
        "write" | "mutate" | "delete" | "network_write"
    )
}

pub(super) fn tool_call_key(input: &ParseInput, name: &str, line_no: u32) -> String {
    format!(
        "tool_call:{}:{}:{line_no}",
        input.document.source_item_key.0, name
    )
}

pub(super) fn warning(input: &ParseInput, code: &str, message: String) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: Some(input.document.source_item_key.clone()),
        retryable: false,
    }
}

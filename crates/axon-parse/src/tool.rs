use axon_api::source::{Severity, SourceParseFacts, SourceWarning};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "tool";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolParseItems {
    pub facts: Vec<SourceParseFacts>,
    pub warnings: Vec<SourceWarning>,
}

pub fn tool_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    tool_parse_items(input).facts
}

pub fn tool_parse_items(input: &ParseInput) -> ToolParseItems {
    let mut parsed = ToolParseItems::default();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let Some(tool) = value
            .get("tool")
            .or_else(|| value.get("tool_name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let action = value
            .get("action")
            .or_else(|| value.get("name"))
            .and_then(Value::as_str);
        let name = action
            .map(|action| format!("{tool}.{action}"))
            .unwrap_or_else(|| tool.to_string());
        let line_no = idx as u32 + 1;
        let output_kind = value.get("output").map(json_kind).unwrap_or("missing");

        parsed.facts.push(source_fact(
            input,
            "tool_output_jsonl",
            "jsonl",
            "tool_output",
            name,
            json!({
                "tool": tool,
                "action": action,
                "status": value.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                "output_kind": output_kind,
            }),
            Some(line_no),
        ));

        for path in redacted_paths(&value) {
            parsed.facts.push(source_fact(
                input,
                "tool_output_jsonl",
                "jsonl_heuristic",
                "tool_redacted_field",
                path.clone(),
                json!({
                    "tool": tool,
                    "action": action,
                    "path": path,
                }),
                Some(line_no),
            ));
            parsed.warnings.push(warning(
                input,
                "tool.redacted_field",
                format!("tool output contains redacted field at {path}"),
            ));
        }

        if let Some(artifact) = output_artifact(&value) {
            parsed.facts.push(source_fact(
                input,
                "tool_output_jsonl",
                "jsonl_heuristic",
                "tool_artifact_ref",
                artifact.artifact_id.clone(),
                json!({
                    "tool": tool,
                    "action": action,
                    "artifact_id": artifact.artifact_id,
                    "uri": artifact.uri,
                    "size_bytes": artifact.size_bytes,
                    "reason": artifact.reason,
                }),
                Some(line_no),
            ));
            parsed.warnings.push(warning(
                input,
                "tool.output_artifact",
                "tool output was stored as an artifact reference".to_string(),
            ));
        }
    }

    parsed
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn redacted_paths(value: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    collect_redacted_paths(value, "", &mut paths);
    paths
}

fn collect_redacted_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::String(text) if is_redacted(text) => {
            paths.push(if path.is_empty() {
                "/".to_string()
            } else {
                path.to_string()
            });
        }
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

#[derive(Debug, Clone)]
struct ArtifactOutput {
    artifact_id: String,
    uri: Option<String>,
    size_bytes: Option<u64>,
    reason: Option<String>,
}

fn output_artifact(value: &Value) -> Option<ArtifactOutput> {
    value
        .get("output")
        .and_then(artifact_from_value)
        .or_else(|| value.get("artifact").and_then(artifact_from_value))
        .or_else(|| artifact_from_value(value))
}

fn artifact_from_value(value: &Value) -> Option<ArtifactOutput> {
    let object = value.as_object()?;
    let artifact_id = string_field(value, &["artifact_id", "output_artifact_id"])?;
    let reason = string_field(value, &["reason", "artifact_reason", "output_reason"]);
    let size_bytes = object
        .get("size_bytes")
        .or_else(|| object.get("bytes"))
        .and_then(Value::as_u64);
    let is_oversized = reason
        .as_deref()
        .is_some_and(|reason| reason.contains("oversized") || reason.contains("large"))
        || size_bytes.is_some_and(|size| size > 64 * 1024)
        || object.contains_key("output_artifact_id");

    is_oversized.then(|| ArtifactOutput {
        artifact_id,
        uri: string_field(value, &["uri", "url", "path", "display_path"]),
        size_bytes,
        reason,
    })
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .find(|text| !text.is_empty())
        .map(str::to_string)
}

fn warning(input: &ParseInput, code: &str, message: String) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: Some(input.document.source_item_key.clone()),
        retryable: false,
    }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;

use axon_api::source::{
    GraphCandidate, LifecycleStatus, Severity, SourceParseFacts, SourceWarning,
};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::candidate_edge;
use crate::parser::{ParseInput, ParseResult, stage_header};

pub const MODULE_NAME: &str = "tool";
pub const MAX_TOOL_JSONL_LINE_BYTES: usize = 256 * 1024;
pub const MAX_TOOL_JSON_DEPTH: usize = 32;
pub const MAX_TOOL_JSON_ENTRIES: usize = 4_096;
pub const MAX_TOOL_REDACTED_FIELDS: usize = 512;
pub const MAX_TOOL_RESOURCES_PER_RECORD: usize = 128;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolParseItems {
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub warnings: Vec<SourceWarning>,
}

pub fn tool_parse_items(input: &ParseInput) -> ToolParseItems {
    let mut parsed = ToolParseItems::default();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let line_no = idx as u32 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_TOOL_JSONL_LINE_BYTES {
            parsed.warnings.push(warning(
                input,
                "tool.jsonl.line_too_large",
                format!("tool JSONL line {line_no} exceeds maximum byte length"),
            ));
            continue;
        }
        let value = match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => value,
            Err(error) => {
                parsed.warnings.push(warning(
                    input,
                    "parse.jsonl.invalid_line",
                    format!("invalid JSONL at line {line_no}: {error}"),
                ));
                continue;
            }
        };
        if !json_within_caps(&value) {
            parsed.warnings.push(warning(
                input,
                "tool.jsonl.bounds_exceeded",
                format!("tool JSONL line {line_no} exceeds structural limits"),
            ));
            continue;
        }
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
        let output_kind = value.get("output").map(json_kind).unwrap_or("missing");
        let side_effect_class = value
            .get("side_effect_class")
            .and_then(Value::as_str)
            .unwrap_or("read");

        parsed.facts.push(source_fact(
            input,
            "tool_output_jsonl",
            "jsonl",
            "tool_observed_claim",
            &name,
            json!({
                "tool": tool,
                "action": action,
                "status": value.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                "output_kind": output_kind,
                "observed_execution_requested": value.get("execution_requested").and_then(Value::as_bool),
                "observed_execution_allowed_claim": value.get("execution_allowed").and_then(Value::as_bool),
                "trusted_policy": false,
                "side_effect_class": side_effect_class,
                "argv": "[redacted]",
                "env": "[redacted]",
                "stdout": "[redacted]",
                "stderr": "[redacted]",
            }),
            Some(line_no),
        ));
        parsed.graph_candidates.push(candidate_edge(
            input,
            "tool_output_jsonl",
            "tool_call_event",
            "tool_call",
            &tool_call_key(input, &name, line_no),
            "tool",
            &format!("tool:{tool}"),
            "tool_call_uses_tool",
            "tool_call_event",
            Some(line_no),
            Some(format!("{name} [redacted]")),
        ));

        for path in redacted_paths(&value)
            .into_iter()
            .take(MAX_TOOL_REDACTED_FIELDS)
        {
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
            let artifact_id = artifact.artifact_id.clone();
            parsed.facts.push(source_fact(
                input,
                "tool_output_jsonl",
                "jsonl_heuristic",
                "tool_artifact_ref",
                artifact_id.clone(),
                json!({
                    "tool": tool,
                    "action": action,
                    "artifact_id": artifact_id,
                    "uri": artifact.uri,
                    "size_bytes": artifact.size_bytes,
                    "reason": artifact.reason,
                }),
                Some(line_no),
            ));
            parsed.graph_candidates.push(candidate_edge(
                input,
                "tool_output_jsonl",
                "tool_result_event",
                "tool_call",
                &tool_call_key(input, &name, line_no),
                "artifact",
                &format!("artifact:{artifact_id}"),
                "tool_call_produced_artifact",
                "tool_result_event",
                Some(line_no),
                Some(format!("artifact:{artifact_id}")),
            ));
            parsed.warnings.push(warning(
                input,
                "tool.output_artifact",
                "tool output was stored as an artifact reference".to_string(),
            ));
        }

        for uri in external_resources(&value)
            .into_iter()
            .take(MAX_TOOL_RESOURCES_PER_RECORD)
        {
            parsed.facts.push(source_fact(
                input,
                "tool_output_jsonl",
                "jsonl_heuristic",
                "external_resource",
                uri.clone(),
                json!({
                    "tool": tool,
                    "action": action,
                    "uri": uri,
                    "side_effect_class": side_effect_class,
                }),
                Some(line_no),
            ));
            parsed.graph_candidates.push(candidate_edge(
                input,
                "tool_output_jsonl",
                "tool_call_event",
                "tool_call",
                &tool_call_key(input, &name, line_no),
                "external_resource",
                &format!("external:{uri}"),
                if mutating_side_effect(side_effect_class) {
                    "tool_call_mutated_resource"
                } else {
                    "tool_call_read_resource"
                },
                "tool_call_event",
                Some(line_no),
                Some(uri),
            ));
        }
    }

    parsed
}

pub fn tool_parse_result(input: &ParseInput) -> ParseResult {
    let parsed = tool_parse_items(input);
    let status = if parsed.warnings.is_empty() {
        LifecycleStatus::Completed
    } else {
        LifecycleStatus::CompletedDegraded
    };
    ParseResult {
        header: stage_header(input, status, parsed.warnings.clone(), None),
        document_id: input.document.document_id.clone(),
        facts: parsed.facts,
        graph_candidates: parsed.graph_candidates,
        parser_id: "tool_output_jsonl".to_string(),
        parser_version: crate::facts::PARSER_VERSION.to_string(),
        warnings: parsed.warnings,
        errors: Vec::new(),
    }
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

fn json_within_caps(value: &Value) -> bool {
    let mut stack = vec![(value, 1usize)];
    let mut entries = 0usize;
    while let Some((value, depth)) = stack.pop() {
        if depth > MAX_TOOL_JSON_DEPTH {
            return false;
        }
        match value {
            Value::Array(items) => {
                entries += items.len();
                if entries > MAX_TOOL_JSON_ENTRIES {
                    return false;
                }
                stack.extend(items.iter().map(|item| (item, depth + 1)));
            }
            Value::Object(object) => {
                entries += object.len();
                if entries > MAX_TOOL_JSON_ENTRIES {
                    return false;
                }
                stack.extend(object.values().map(|item| (item, depth + 1)));
            }
            _ => {}
        }
    }
    true
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

fn external_resources(value: &Value) -> Vec<String> {
    let Some(resources) = value.get("resources").and_then(Value::as_array) else {
        return Vec::new();
    };
    resources
        .iter()
        .filter_map(|resource| resource.get("uri").and_then(Value::as_str))
        .filter(|uri| !uri.is_empty())
        .map(str::to_string)
        .collect()
}

fn mutating_side_effect(side_effect_class: &str) -> bool {
    matches!(
        side_effect_class,
        "write" | "mutate" | "delete" | "network_write"
    )
}

fn tool_call_key(input: &ParseInput, name: &str, line_no: u32) -> String {
    format!(
        "tool_call:{}:{}:{line_no}",
        input.document.source_item_key.0, name
    )
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

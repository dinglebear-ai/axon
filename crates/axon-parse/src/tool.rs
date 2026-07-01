use axon_api::source::SourceParseFacts;
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "tool";

pub fn tool_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let value = serde_json::from_str::<Value>(line.trim()).ok()?;
            let tool = value
                .get("tool")
                .or_else(|| value.get("tool_name"))
                .and_then(Value::as_str)?;
            let action = value
                .get("action")
                .or_else(|| value.get("name"))
                .and_then(Value::as_str);
            let name = action
                .map(|action| format!("{tool}.{action}"))
                .unwrap_or_else(|| tool.to_string());
            let output_kind = value.get("output").map(json_kind).unwrap_or("missing");
            Some(source_fact(
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
                Some(idx as u32 + 1),
            ))
        })
        .collect()
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

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;

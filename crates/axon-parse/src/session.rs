use axon_api::source::SourceParseFacts;
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact, turn_range};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "session";

pub fn session_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let value = serde_json::from_str::<Value>(line.trim()).ok()?;
            let role = value
                .get("role")
                .and_then(Value::as_str)
                .or_else(|| value.get("speaker").and_then(Value::as_str))
                .unwrap_or("unknown");
            let mut fact = source_fact(
                input,
                "session_jsonl",
                "jsonl",
                "session_turn",
                role,
                json!({
                    "type": value.get("type").and_then(Value::as_str).unwrap_or("message"),
                    "role": role,
                    "has_content": value.get("content").is_some(),
                }),
                Some(idx as u32 + 1),
            );
            fact.range = Some(turn_range(idx as u32 + 1, (idx + 1).to_string()));
            Some(fact)
        })
        .collect()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

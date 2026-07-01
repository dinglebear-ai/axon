use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact, turn_range};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "session";

pub fn session_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    session_items(input).0
}

pub fn session_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let line_no = idx as u32 + 1;
        let turn_id = (idx + 1).to_string();
        let role = value
            .get("role")
            .and_then(Value::as_str)
            .or_else(|| value.get("speaker").and_then(Value::as_str))
            .unwrap_or("unknown");

        let mut turn_fact = source_fact(
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
            Some(line_no),
        );
        turn_fact.range = Some(turn_range(line_no, turn_id.clone()));
        facts.push(turn_fact);

        append_invocations(
            input,
            &mut facts,
            &mut candidates,
            &value,
            InvocationSpec {
                keys: &["tool_calls", "tool_call", "tools", "tool"],
                kind: "session_tool_call",
                name_keys: &["name", "tool", "tool_name"],
            },
            line_no,
            &turn_id,
            trimmed,
        );
        append_invocations(
            input,
            &mut facts,
            &mut candidates,
            &value,
            InvocationSpec {
                keys: &["skills", "skill", "skills_invoked", "skill_invocations"],
                kind: "session_skill_invocation",
                name_keys: &["name", "skill", "skill_name"],
            },
            line_no,
            &turn_id,
            trimmed,
        );
        append_invocations(
            input,
            &mut facts,
            &mut candidates,
            &value,
            InvocationSpec {
                keys: &[
                    "agents",
                    "agent",
                    "agents_invoked",
                    "agent_invocations",
                    "subagents",
                ],
                kind: "session_agent_invocation",
                name_keys: &["name", "agent", "agent_name", "subagent"],
            },
            line_no,
            &turn_id,
            trimmed,
        );
    }

    (facts, candidates)
}

struct InvocationSpec<'a> {
    keys: &'a [&'a str],
    kind: &'a str,
    name_keys: &'a [&'a str],
}

fn append_invocations(
    input: &ParseInput,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
    value: &Value,
    spec: InvocationSpec<'_>,
    line_no: u32,
    turn_id: &str,
    quote: &str,
) {
    for key in spec.keys {
        let Some(field_value) = value.get(*key) else {
            continue;
        };
        for invocation in invocations_from_value(field_value, *key, spec.name_keys) {
            let mut fact = source_fact(
                input,
                "session_jsonl",
                "jsonl_heuristic",
                spec.kind,
                invocation.name.clone(),
                json!({
                    "field": invocation.field,
                    "turn_id": turn_id,
                    "call_id": invocation.call_id,
                    "action": invocation.action,
                }),
                Some(line_no),
            );
            fact.range = Some(turn_range(line_no, turn_id.to_string()));
            facts.push(fact);
            candidates.push(graph_candidate(
                input,
                "session_jsonl",
                spec.kind,
                &invocation.name,
                Some(line_no),
                Some(quote.to_string()),
            ));
        }
    }
}

struct Invocation {
    name: String,
    field: String,
    call_id: Option<String>,
    action: Option<String>,
}

fn invocations_from_value(value: &Value, field: &str, name_keys: &[&str]) -> Vec<Invocation> {
    match value {
        Value::String(name) if !name.is_empty() => vec![Invocation {
            name: name.clone(),
            field: field.to_string(),
            call_id: None,
            action: None,
        }],
        Value::Array(items) => items
            .iter()
            .flat_map(|item| invocations_from_value(item, field, name_keys))
            .collect(),
        Value::Object(object) => {
            let action = object
                .get("action")
                .or_else(|| object.get("subaction"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let name = name_keys
                .iter()
                .filter_map(|key| object.get(*key).and_then(Value::as_str))
                .find(|name| !name.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    object
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .map(|name| match action.as_deref() {
                    Some(action) if !name.contains('.') => format!("{name}.{action}"),
                    _ => name,
                });
            name.into_iter()
                .map(|name| Invocation {
                    name,
                    field: field.to_string(),
                    call_id: object
                        .get("id")
                        .or_else(|| object.get("call_id"))
                        .or_else(|| object.get("tool_call_id"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    action: action.clone(),
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

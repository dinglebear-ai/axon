use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::candidate_edge;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "env";

pub fn env_example_parse_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let Some((key, value)) = parse_assignment(line) else {
            continue;
        };
        let line_no = idx as u32 + 1;
        let secret = is_secret_key(key) || value_suggests_secret(value);
        let fact_kind = if secret {
            "secret_reference"
        } else {
            "environment_variable"
        };
        facts.push(source_fact(
            input,
            "env_example",
            "line_heuristic",
            fact_kind,
            key,
            json!({
                "key": key,
                "has_default": !value.trim().is_empty(),
                "value_redacted": !value.trim().is_empty(),
            }),
            Some(line_no),
        ));
        candidates.push(candidate_edge(
            input,
            "env_example",
            "env_example",
            "local_checkout",
            &local_checkout_key(input),
            if secret {
                "secret_reference"
            } else {
                "environment_variable"
            },
            &format!("{}:{key}", if secret { "secret" } else { "env" }),
            "repo_declares_env_var",
            "env_example",
            Some(line_no),
            Some(format!("{key}=<redacted>")),
        ));
    }
    (facts, candidates)
}

pub fn env_example_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    env_example_parse_items(input).0
}

fn parse_assignment(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some((key, value))
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "secret",
        "password",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "database_url",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn value_suggests_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("sk-")
        || lower.contains("://") && lower.contains('@') && lower.contains(':')
        || lower.contains("password=")
        || lower.contains("token=")
}

fn local_checkout_key(input: &ParseInput) -> String {
    format!("local://{}", input.document.source_id.0)
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;

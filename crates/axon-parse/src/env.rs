use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "env";

pub fn env_example_parse_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    (env_example_facts(input), Vec::new())
}

pub fn env_example_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let (key, value) = trimmed.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let fact_kind = if is_secret_key(key) {
                "secret_reference"
            } else {
                "env_var"
            };
            Some(source_fact(
                input,
                "env_example",
                "line_heuristic",
                fact_kind,
                key,
                json!({
                    "has_default": !value.trim().is_empty(),
                }),
                Some(idx as u32 + 1),
            ))
        })
        .collect()
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
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;

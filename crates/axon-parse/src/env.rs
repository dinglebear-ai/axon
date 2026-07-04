use axon_api::source::SourceParseFacts;
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "env";

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
            Some(source_fact(
                input,
                "env_example",
                "line_heuristic",
                "env_var",
                key,
                json!({
                    "has_default": !value.trim().is_empty(),
                }),
                Some(idx as u32 + 1),
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;

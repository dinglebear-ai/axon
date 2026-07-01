use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "code";

pub fn symbol_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    symbol_facts_with_graph(input).0
}

pub fn symbol_facts_with_graph(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim_start();
        let Some((language, symbol_kind, name)) =
            rust_symbol(trimmed).or_else(|| python_symbol(trimmed))
        else {
            continue;
        };
        let line_number = idx as u32 + 1;
        facts.push(source_fact(
            input,
            "code_symbols",
            "line_heuristic",
            "code_symbol",
            name.clone(),
            json!({
                "language": language,
                "symbol_kind": symbol_kind,
            }),
            Some(line_number),
        ));
        candidates.push(graph_candidate(
            input,
            "code_symbols",
            "code_symbol",
            &name,
            Some(line_number),
            Some(trimmed.to_string()),
        ));
    }

    (facts, candidates)
}

fn rust_symbol(line: &str) -> Option<(&'static str, &'static str, String)> {
    let line = line.strip_prefix("pub ").unwrap_or(line);
    for (prefix, kind) in [
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("fn ", "function"),
        ("async fn ", "function"),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(("rust", kind, take_identifier(rest)));
        }
    }
    None
}

fn python_symbol(line: &str) -> Option<(&'static str, &'static str, String)> {
    for (prefix, kind) in [("class ", "class"), ("def ", "function")] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(("python", kind, take_identifier(rest)));
        }
    }
    None
}

fn take_identifier(rest: &str) -> String {
    rest.chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;

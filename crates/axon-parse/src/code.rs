use axon_api::source::SourceParseFacts;
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "code";

pub fn symbol_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim_start();
            rust_symbol(trimmed).or_else(|| python_symbol(trimmed)).map(
                |(language, symbol_kind, name)| {
                    source_fact(
                        input,
                        "code_symbols",
                        "line_heuristic",
                        "code_symbol",
                        name,
                        json!({
                            "language": language,
                            "symbol_kind": symbol_kind,
                        }),
                        Some(idx as u32 + 1),
                    )
                },
            )
        })
        .collect()
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

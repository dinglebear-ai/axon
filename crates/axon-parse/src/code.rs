use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact_ranged, span_range};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "code";

/// `axon-parse` has no tree-sitter/AST grammar wired in yet (tracked as
/// deferred — see the crate CLAUDE.md and parsing-contract.md "AST and
/// Structural Parsing"; tree-sitter is only a direct dependency of
/// `axon-vector` today, not a shared workspace dependency `axon-parse` can
/// pull in without adding a new heavy dependency edge). Every code symbol
/// fact below is produced by a line/indentation heuristic, not a grammar, so
/// every fact is stamped `parser_method = "regex_fallback"`,
/// `confidence < 0.75`, and `symbol_extraction_status = "heuristic_fallback"`
/// per the contract's fallback requirements.
pub const PARSER_METHOD: &str = "regex_fallback";
const MAX_SYMBOL_SCAN_LINES: usize = 4_000;

pub fn symbol_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    symbol_facts_with_graph(input).0
}

pub fn symbol_facts_with_graph(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let lines: Vec<&str> = inline_text(input).lines().collect();

    // Indentation-based parent tracking: a symbol nested under a
    // shallower-indented symbol (e.g. a Python method under its class) is
    // recorded as that symbol's child via `parent_symbol`.
    let mut parent_stack: Vec<(usize, String)> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let indent = indent_of(line);
        let trimmed = line.trim_start();
        if is_comment_or_string_line(trimmed) {
            continue;
        }
        let Some((language, symbol_kind, name, visibility)) =
            rust_symbol(trimmed).or_else(|| python_symbol(trimmed))
        else {
            continue;
        };

        while parent_stack
            .last()
            .is_some_and(|(parent_indent, _)| *parent_indent >= indent)
        {
            parent_stack.pop();
        }
        let parent_symbol = parent_stack.last().map(|(_, name)| name.clone());

        let line_start = idx as u32 + 1;
        let (line_end, truncated) = match language {
            "rust" => rust_symbol_end(&lines, idx),
            _ => python_symbol_end(&lines, idx, indent),
        };

        facts.push(source_fact_ranged(
            input,
            "code_symbols",
            PARSER_METHOD,
            "code_symbol",
            name.clone(),
            json!({
                "language": language,
                "symbol_kind": symbol_kind,
                "symbol_visibility": visibility,
                "parent_symbol": parent_symbol,
                "symbol_extraction_status": "heuristic_fallback",
                "code_symbol_range_truncated": truncated,
            }),
            Some(span_range(line_start, line_end)),
        ));
        candidates.push(graph_candidate(
            input,
            "code_symbols",
            "code_symbol",
            &name,
            Some(line_start),
            Some(trimmed.to_string()),
        ));

        parent_stack.push((indent, name));
    }

    (facts, candidates)
}

fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .count()
}

/// Best-effort brace-depth scan for a Rust symbol's closing line. Comments
/// and string literals containing braces are not excluded (a true fallback
/// limitation — the reason this is `regex_fallback`, not an AST parser), and
/// the scan is capped at `MAX_SYMBOL_SCAN_LINES` to bound cost on huge files;
/// `truncated` reports when the cap was hit so callers/downstream chunkers
/// know the range may not cover the whole symbol.
fn rust_symbol_end(lines: &[&str], start_idx: usize) -> (u32, bool) {
    let start_line = lines[start_idx];
    if !start_line.contains('{') && start_line.trim_end().ends_with(';') {
        return (start_idx as u32 + 1, false);
    }

    let limit = lines.len().min(start_idx + MAX_SYMBOL_SCAN_LINES);
    let mut depth: i32 = 0;
    let mut seen_open = false;
    for (offset, line) in lines[start_idx..limit].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if seen_open && depth <= 0 {
            return (start_idx as u32 + offset as u32 + 1, false);
        }
    }
    (limit as u32, limit < lines.len())
}

/// Best-effort indentation scan for a Python symbol's body extent: the body
/// continues while subsequent non-blank lines are indented deeper than the
/// declaration. Capped the same way as `rust_symbol_end`.
fn python_symbol_end(lines: &[&str], start_idx: usize, declared_indent: usize) -> (u32, bool) {
    let limit = lines.len().min(start_idx + MAX_SYMBOL_SCAN_LINES);
    let mut end_idx = start_idx;
    for (idx, line) in lines.iter().enumerate().take(limit).skip(start_idx + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if indent_of(line) <= declared_indent {
            break;
        }
        end_idx = idx;
    }
    (
        end_idx as u32 + 1,
        limit < lines.len() && end_idx + 1 == limit,
    )
}

fn rust_symbol(line: &str) -> Option<(&'static str, &'static str, String, &'static str)> {
    let visibility = if line.starts_with("pub ") {
        "public"
    } else {
        "private"
    };
    let line = line.strip_prefix("pub ").unwrap_or(line);
    for (prefix, kind) in [
        ("struct ", "struct"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("fn ", "function"),
        ("async fn ", "function"),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(("rust", kind, take_identifier(rest), visibility));
        }
    }
    None
}

fn python_symbol(line: &str) -> Option<(&'static str, &'static str, String, &'static str)> {
    for (prefix, kind) in [("class ", "class"), ("def ", "function")] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let name = take_identifier(rest);
            let visibility = if name.starts_with('_') {
                "private"
            } else {
                "public"
            };
            return Some(("python", kind, name, visibility));
        }
    }
    None
}

fn take_identifier(rest: &str) -> String {
    rest.chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect()
}

fn is_comment_or_string_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.starts_with("\"")
        || line.starts_with('\'')
        || line.starts_with("r#\"")
        || line.starts_with("r\"")
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;

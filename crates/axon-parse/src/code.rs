use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact_ranged, span_range};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

mod ast;

pub const MODULE_NAME: &str = "code";
pub const AST_PARSER_METHOD: &str = "tree_sitter";
pub const FALLBACK_PARSER_METHOD: &str = "regex_fallback";
const MAX_SYMBOL_SCAN_LINES: usize = 4_000;

pub fn symbol_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    symbol_facts_with_graph(input).0
}

pub fn symbol_facts_with_graph(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    if let Ok(symbols) = ast::parse_symbols(input) {
        return ast::facts_with_graph(input, symbols);
    }
    fallback_symbol_facts_with_graph(input)
}

fn fallback_symbol_facts_with_graph(
    input: &ParseInput,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let lines: Vec<&str> = inline_text(input).lines().collect();
    let js_ts_language = js_ts_language(input);

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
            symbol_for_line(trimmed, js_ts_language)
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
            "rust" | "javascript" | "typescript" => brace_symbol_end(&lines, idx),
            _ => indentation_symbol_end(&lines, idx, indent),
        };

        facts.push(source_fact_ranged(
            input,
            "code_symbols",
            FALLBACK_PARSER_METHOD,
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

/// Best-effort brace-depth scan for a Rust/JS/TS symbol's closing line. Comments
/// and string literals containing braces are not excluded (a true fallback
/// limitation — the reason this is `regex_fallback`, not an AST parser), and
/// the scan is capped at `MAX_SYMBOL_SCAN_LINES` to bound cost on huge files;
/// `truncated` reports when the cap was hit so callers/downstream chunkers
/// know the range may not cover the whole symbol.
fn brace_symbol_end(lines: &[&str], start_idx: usize) -> (u32, bool) {
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
/// declaration. Capped the same way as `brace_symbol_end`.
fn indentation_symbol_end(lines: &[&str], start_idx: usize, declared_indent: usize) -> (u32, bool) {
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

fn symbol_for_line(
    line: &str,
    js_ts_language: Option<&'static str>,
) -> Option<(&'static str, &'static str, String, &'static str)> {
    if let Some(language) = js_ts_language {
        rust_symbol(line)
            .or_else(|| js_ts_symbol(line, language))
            .or_else(|| python_symbol(line))
    } else {
        rust_symbol(line).or_else(|| python_symbol(line))
    }
}

fn js_ts_symbol(
    line: &str,
    language: &'static str,
) -> Option<(&'static str, &'static str, String, &'static str)> {
    let (line, visibility) = strip_js_ts_modifiers(line);
    for (prefix, kind) in [
        ("async function ", "function"),
        ("function ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
        ("type ", "type"),
        ("enum ", "enum"),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((language, kind, take_identifier(rest), visibility));
        }
    }
    js_ts_assignment_symbol(line).map(|(kind, name)| (language, kind, name, visibility))
}

fn strip_js_ts_modifiers(line: &str) -> (&str, &'static str) {
    let mut rest = line;
    let mut visibility = "private";
    loop {
        if let Some(stripped) = rest.strip_prefix("export default ") {
            rest = stripped;
            visibility = "public";
        } else if let Some(stripped) = rest.strip_prefix("export ") {
            rest = stripped;
            visibility = "public";
        } else if let Some(stripped) = rest.strip_prefix("declare ") {
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("abstract ") {
            rest = stripped;
        } else {
            break;
        }
    }
    (rest, visibility)
}

fn js_ts_assignment_symbol(line: &str) -> Option<(&'static str, String)> {
    let rest = line
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("let "))
        .or_else(|| line.strip_prefix("var "))?;
    let name = take_identifier(rest);
    if name.is_empty() {
        return None;
    }
    let rhs = rest.get(name.len()..)?.trim_start();
    let kind = if rhs.contains("=>") || rhs.contains("function") || rhs.contains("React.FC") {
        "function"
    } else {
        "constant"
    };
    Some((kind, name))
}

fn js_ts_language(input: &ParseInput) -> Option<&'static str> {
    let language = input
        .document
        .language
        .as_deref()
        .map(str::to_ascii_lowercase);
    if language
        .as_deref()
        .is_some_and(|value| value.contains("typescript") || value == "ts" || value == "tsx")
    {
        return Some("typescript");
    }
    if language.as_deref().is_some_and(|value| {
        value.contains("javascript") || value == "js" || value == "jsx" || value == "mjs"
    }) {
        return Some("javascript");
    }
    let path = input
        .document
        .path
        .as_deref()
        .or_else(|| Some(input.document.canonical_uri.as_str()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".mts")
        || path.ends_with(".cts")
    {
        Some("typescript")
    } else if path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
    {
        Some("javascript")
    } else {
        None
    }
}

fn take_identifier(rest: &str) -> String {
    rest.chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
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

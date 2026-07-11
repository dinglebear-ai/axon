//! Example-validation pass for `docs check`
//! (`docs-generator-contract.md` "Example Validation").
//!
//! Generated reference docs under `docs/reference/**/*.md` may embed fenced
//! `json`/`toml` example payloads that should validate against a JSON Schema
//! artifact. A payload opts in with a marker comment on the line immediately
//! before its fence:
//!
//! ```text
//! <!-- doc-example: kind=json schema=config/config.schema.json -->
//! ```
//!
//! followed directly by a ` ```json ` or ` ```toml ` fence. `schema` is a path
//! relative to `docs/reference/`. Fences with no marker are ignored — this is
//! opt-in, not a blanket "validate every fence" pass. TOML bodies are
//! converted to JSON before validation; JSON bodies are parsed directly.
//! Compiled schema validators are cached per schema path for the run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use jsonschema::Validator;
use serde_json::Value;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExampleKind {
    Json,
    Toml,
}

impl ExampleKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Toml => "toml",
        }
    }
}

/// One marker-annotated fenced example found in a markdown doc. `body` is
/// `Err` when the marker itself is malformed or the fence it points at is
/// missing/mismatched/unterminated — those are reported as failures without
/// ever attempting to parse or schema-validate anything.
#[derive(Debug, Clone)]
struct MarkedExample {
    doc_rel_path: String,
    marker_line: usize,
    kind: ExampleKind,
    schema_rel_path: String,
    body: Result<String, String>,
}

pub fn check(root: &Path) -> Result<()> {
    let docs_root = root.join("docs/reference");
    if !docs_root.is_dir() {
        println!("docs check (examples): no docs/reference tree found; nothing to validate.");
        return Ok(());
    }

    let examples = collect_examples(root, &docs_root)?;
    if examples.is_empty() {
        println!("docs check (examples): no marked examples found.");
        return Ok(());
    }

    let mut cache: HashMap<String, Validator> = HashMap::new();
    let mut failures = Vec::new();
    for example in &examples {
        if let Err(msg) = validate_example(&docs_root, example, &mut cache) {
            failures.push(format!(
                "{} (line {}): {msg}",
                example.doc_rel_path, example.marker_line
            ));
        }
    }

    if !failures.is_empty() {
        bail!(
            "docs check (examples): {} example(s) failed validation:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    println!(
        "docs check (examples): {} marked example(s) validated.",
        examples.len()
    );
    Ok(())
}

fn collect_examples(root: &Path, docs_root: &Path) -> Result<Vec<MarkedExample>> {
    let mut examples = Vec::new();
    for entry in WalkDir::new(docs_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        examples.extend(parse_markers(&content, &rel(root, path)));
    }
    examples.sort_by(|a, b| {
        a.doc_rel_path
            .cmp(&b.doc_rel_path)
            .then(a.marker_line.cmp(&b.marker_line))
    });
    Ok(examples)
}

/// Scan `content` line by line for `doc-example` marker comments, pairing
/// each with the fence that immediately follows it.
fn parse_markers(content: &str, doc_rel_path: &str) -> Vec<MarkedExample> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if !is_marker_line(lines[i]) {
            i += 1;
            continue;
        }
        let marker_line = i + 1;
        match parse_marker_line(lines[i]) {
            Ok((kind, schema_rel_path)) => {
                let fence_open_idx = i + 1;
                let (body, next_idx) = extract_fence_body(&lines, fence_open_idx, kind);
                out.push(MarkedExample {
                    doc_rel_path: doc_rel_path.to_string(),
                    marker_line,
                    kind,
                    schema_rel_path,
                    body,
                });
                i = next_idx.max(fence_open_idx);
            }
            Err(msg) => {
                out.push(MarkedExample {
                    doc_rel_path: doc_rel_path.to_string(),
                    marker_line,
                    kind: ExampleKind::Json,
                    schema_rel_path: String::new(),
                    body: Err(msg),
                });
            }
        }
        i += 1;
    }
    out
}

fn is_marker_line(line: &str) -> bool {
    line.trim().starts_with("<!-- doc-example:")
}

/// Parse `<!-- doc-example: kind=<json|toml> schema=<path> -->` into its
/// `(kind, schema)` pair, or an error describing what's malformed.
fn parse_marker_line(line: &str) -> Result<(ExampleKind, String), String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix("<!--")
        .and_then(|s| s.strip_suffix("-->"))
        .map(str::trim)
        .and_then(|s| s.strip_prefix("doc-example:"))
        .map(str::trim)
        .ok_or_else(|| format!("malformed doc-example marker comment: `{trimmed}`"))?;

    let mut kind = None;
    let mut schema = None;
    for token in inner.split_whitespace() {
        if let Some(value) = token.strip_prefix("kind=") {
            kind = Some(ExampleKind::parse(value).ok_or_else(|| {
                format!("doc-example marker has unknown kind `{value}` (expected `json` or `toml`)")
            })?);
        } else if let Some(value) = token.strip_prefix("schema=") {
            schema = Some(value.to_string());
        }
    }
    let kind = kind.ok_or_else(|| "doc-example marker is missing `kind=`".to_string())?;
    let schema = schema.ok_or_else(|| "doc-example marker is missing `schema=`".to_string())?;
    Ok((kind, schema))
}

/// Extract the body of the fence starting at `fence_open_idx`, validating
/// that it opens with a fence tagged `kind` and later closes. Returns the
/// body (or a structural error) plus the index of the closing fence line to
/// resume scanning from (or `fence_open_idx` itself when no fence was found).
fn extract_fence_body(
    lines: &[&str],
    fence_open_idx: usize,
    kind: ExampleKind,
) -> (Result<String, String>, usize) {
    let Some(open_line) = lines.get(fence_open_idx) else {
        return (
            Err("doc-example marker not followed by a fenced code block (end of file)".to_string()),
            fence_open_idx,
        );
    };
    let Some(lang) = open_line.trim().strip_prefix("```") else {
        return (
            Err(format!(
                "doc-example marker not immediately followed by a fenced code block (found: `{}`)",
                open_line.trim()
            )),
            fence_open_idx,
        );
    };
    if lang.is_empty() {
        return (
            Err("doc-example fence is missing a language tag".to_string()),
            fence_open_idx,
        );
    }
    if lang != kind.as_str() {
        return (
            Err(format!(
                "doc-example kind `{}` does not match fence language `{lang}`",
                kind.as_str()
            )),
            fence_open_idx,
        );
    }

    let mut j = fence_open_idx + 1;
    while j < lines.len() && lines[j].trim() != "```" {
        j += 1;
    }
    if j >= lines.len() {
        return (
            Err("doc-example fenced code block is unterminated".to_string()),
            fence_open_idx,
        );
    }
    (Ok(lines[fence_open_idx + 1..j].join("\n")), j)
}

fn validate_example(
    docs_root: &Path,
    example: &MarkedExample,
    cache: &mut HashMap<String, Validator>,
) -> Result<(), String> {
    let body = example.body.as_ref().map_err(Clone::clone)?;
    let value = parse_body(example.kind, body)?;
    let validator = get_or_build_validator(docs_root, &example.schema_rel_path, cache)?;
    validator.validate(&value).map_err(|err| {
        format!(
            "failed schema validation against `{}`: {err}",
            example.schema_rel_path
        )
    })
}

fn parse_body(kind: ExampleKind, body: &str) -> Result<Value, String> {
    match kind {
        ExampleKind::Json => {
            serde_json::from_str(body).map_err(|err| format!("invalid JSON: {err}"))
        }
        ExampleKind::Toml => {
            let toml_value: toml::Value =
                toml::from_str(body).map_err(|err| format!("invalid TOML: {err}"))?;
            serde_json::to_value(toml_value)
                .map_err(|err| format!("failed to convert TOML to JSON for validation: {err}"))
        }
    }
}

fn get_or_build_validator<'a>(
    docs_root: &Path,
    schema_rel_path: &str,
    cache: &'a mut HashMap<String, Validator>,
) -> Result<&'a Validator, String> {
    if !cache.contains_key(schema_rel_path) {
        let schema_path: PathBuf = docs_root.join(schema_rel_path);
        let content = std::fs::read_to_string(&schema_path).map_err(|err| {
            format!("schema `{schema_rel_path}` not found under docs/reference: {err}")
        })?;
        let schema_value: Value = serde_json::from_str(&content)
            .map_err(|err| format!("schema `{schema_rel_path}` is not valid JSON: {err}"))?;
        let validator = jsonschema::validator_for(&schema_value)
            .map_err(|err| format!("schema `{schema_rel_path}` failed to compile: {err}"))?;
        cache.insert(schema_rel_path.to_string(), validator);
    }
    Ok(cache
        .get(schema_rel_path)
        .expect("validator was just inserted for this schema path"))
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;

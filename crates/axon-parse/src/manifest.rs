use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "manifest";

pub fn dependency_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let path = input.document.path.as_deref().unwrap_or_default();
    let deps = if path.ends_with("Cargo.toml") {
        cargo_deps(input)
    } else if path.ends_with("package.json") {
        package_json_deps(input)
    } else if path.ends_with("requirements.txt") {
        requirements_deps(input)
    } else if path.ends_with("pyproject.toml") {
        pyproject_deps(input)
    } else {
        Vec::new()
    };

    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    for dep in deps {
        facts.push(source_fact(
            input,
            dep.parser_id,
            "compact_manifest",
            "dependency",
            dep.name.clone(),
            json!({
                "ecosystem": dep.ecosystem,
                "version": dep.version,
                "scope": dep.scope,
            }),
            Some(dep.line),
        ));
        candidates.push(graph_candidate(
            input,
            dep.parser_id,
            "manifest_dependency",
            &dep.name,
            Some(dep.line),
            Some(dep.quote),
        ));
    }
    (facts, candidates)
}

struct Dep {
    parser_id: &'static str,
    ecosystem: &'static str,
    scope: &'static str,
    name: String,
    version: Option<String>,
    line: u32,
    quote: String,
}

fn cargo_deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    let mut scope: Option<&'static str> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            scope = match trimmed {
                "[dependencies]" => Some("dependencies"),
                "[dev-dependencies]" => Some("dev-dependencies"),
                "[build-dependencies]" => Some("build-dependencies"),
                _ => None,
            };
            continue;
        }
        let Some(scope) = scope else { continue };
        let Some((name, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        deps.push(Dep {
            parser_id: "cargo_manifest",
            ecosystem: "cargo",
            scope,
            name: name.to_string(),
            version: first_quoted(rest),
            line: idx as u32 + 1,
            quote: trimmed.to_string(),
        });
    }
    deps
}

fn package_json_deps(input: &ParseInput) -> Vec<Dep> {
    let Ok(root) = serde_json::from_str::<Value>(inline_text(input)) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for scope in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(obj) = root.get(scope).and_then(Value::as_object) else {
            continue;
        };
        for (name, version) in obj {
            deps.push(Dep {
                parser_id: "package_json",
                ecosystem: "npm",
                scope,
                name: name.clone(),
                version: version.as_str().map(ToOwned::to_owned),
                line: 1,
                quote: format!("{name}: {version}"),
            });
        }
    }
    deps
}

fn requirements_deps(input: &ParseInput) -> Vec<Dep> {
    inline_text(input)
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let split_at = trimmed
                .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
                .unwrap_or(trimmed.len());
            let name = trimmed[..split_at].trim();
            if name.is_empty() {
                return None;
            }
            Some(Dep {
                parser_id: "requirements_txt",
                ecosystem: "python",
                scope: "runtime",
                name: name.to_string(),
                version: (split_at < trimmed.len()).then(|| trimmed[split_at..].to_string()),
                line: idx as u32 + 1,
                quote: trimmed.to_string(),
            })
        })
        .collect()
}

fn pyproject_deps(input: &ParseInput) -> Vec<Dep> {
    let mut deps = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dependencies") {
            continue;
        }
        for quoted in quoted_values(trimmed) {
            let split_at = quoted
                .find(|ch: char| ['=', '<', '>', '!', '~', '['].contains(&ch))
                .unwrap_or(quoted.len());
            deps.push(Dep {
                parser_id: "pyproject_toml",
                ecosystem: "python",
                scope: "project.dependencies",
                name: quoted[..split_at].to_string(),
                version: (split_at < quoted.len()).then(|| quoted[split_at..].to_string()),
                line: idx as u32 + 1,
                quote: quoted,
            });
        }
    }
    deps
}

fn first_quoted(value: &str) -> Option<String> {
    quoted_values(value).into_iter().next()
}

fn quoted_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = value.char_indices();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' && ch != '\'' {
            continue;
        }
        let quote = ch;
        if let Some((end, _)) = chars.by_ref().find(|(_, candidate)| *candidate == quote) {
            values.push(value[start + ch.len_utf8()..end].to_string());
        }
    }
    values
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;

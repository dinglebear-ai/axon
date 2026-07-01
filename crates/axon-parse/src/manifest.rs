use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::source_fact;
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "manifest";

#[path = "manifest/cargo.rs"]
mod cargo;
#[path = "manifest/go.rs"]
mod go;
#[path = "manifest/maven.rs"]
mod maven;
#[path = "manifest/npm.rs"]
mod npm;
#[path = "manifest/python.rs"]
mod python;
#[path = "manifest/yaml_iac.rs"]
mod yaml_iac;

pub fn dependency_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let path = input.document.path.as_deref().unwrap_or_default();
    let deps = if path.ends_with("Cargo.toml") {
        cargo::deps(input)
    } else if path.ends_with("package.json") {
        npm::deps(input)
    } else if path.ends_with("requirements.txt") {
        python::requirements_deps(input)
    } else if path.ends_with("pyproject.toml") {
        python::pyproject_deps(input)
    } else if path.ends_with("go.mod") {
        go::deps(input)
    } else if path.ends_with("pom.xml") {
        maven::deps(input)
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
    for resource in yaml_iac::resources(input) {
        facts.push(source_fact(
            input,
            "yaml_iac_manifest",
            "yaml_iac_heuristic",
            "iac_resource",
            resource.name.clone(),
            json!({
                "api_version": resource.api_version,
                "kind": resource.kind,
                "resource_name": resource.resource_name,
            }),
            Some(resource.line),
        ));
        candidates.push(graph_candidate(
            input,
            "yaml_iac_manifest",
            "iac_resource",
            &resource.name,
            Some(resource.line),
            Some(resource.quote),
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

struct IacResource {
    name: String,
    api_version: String,
    kind: String,
    resource_name: String,
    line: u32,
    quote: String,
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

fn dependency_blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<dependency>") {
        rest = &rest[start..];
        let Some(end) = rest.find("</dependency>") else {
            break;
        };
        let block_end = end + "</dependency>".len();
        blocks.push(&rest[..block_end]);
        rest = &rest[block_end..];
    }
    blocks
}

fn tag_value<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let value = text.split(&open).nth(1)?.split(&close).next()?.trim();
    (!value.is_empty()).then_some(value)
}

fn line_for_offset(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as u32
        + 1
}

fn compact_quote(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn yaml_scalar<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let value = line.strip_prefix(key)?.strip_prefix(':')?.trim();
    let value = value.trim_matches('"').trim_matches('\'');
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;

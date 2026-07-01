use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "schema";

pub fn api_schema_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let path = input.document.path.as_deref().unwrap_or_default();
    if path.ends_with(".graphql") || inline_text(input).contains("type Query") {
        (graphql_facts(input), Vec::new())
    } else if path.ends_with(".proto") || inline_text(input).contains("service ") {
        (proto_facts(input), Vec::new())
    } else {
        openapi_facts(input)
    }
}

fn openapi_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let mut current_path: Option<String> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('/') && trimmed.ends_with(':') {
            current_path = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        if let Some(method) = trimmed
            .strip_suffix(':')
            .filter(|method| ["get", "post", "put", "patch", "delete"].contains(method))
        {
            let Some(path) = current_path.as_deref() else {
                continue;
            };
            let name = format!("{} {path}", method.to_ascii_uppercase());
            facts.push(source_fact(
                input,
                "openapi_schema",
                "yaml_heuristic",
                "api_endpoint",
                name.clone(),
                json!({ "method": method, "path": path }),
                Some(idx as u32 + 1),
            ));
            candidates.push(graph_candidate(
                input,
                "openapi_schema",
                "api_endpoint",
                &name,
                Some(idx as u32 + 1),
                Some(trimmed.to_string()),
            ));
        }
    }
    (facts, candidates)
}

fn graphql_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    let mut facts = Vec::new();
    let mut current_type: Option<String> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("type ") {
            let name = rest
                .split(|ch: char| ch.is_whitespace() || ch == '{')
                .next()
                .unwrap_or_default();
            if !name.is_empty() {
                current_type = Some(name.to_string());
                facts.push(source_fact(
                    input,
                    "graphql_schema",
                    "line_heuristic",
                    "graphql_type",
                    name,
                    json!({ "type_kind": "type" }),
                    Some(idx as u32 + 1),
                ));
            }
        } else if let Some(parent) = current_type.as_deref() {
            if trimmed == "}" {
                current_type = None;
                continue;
            }
            if let Some((field, _)) = trimmed.split_once(':') {
                let field_name = field.split('(').next().unwrap_or(field).trim();
                facts.push(source_fact(
                    input,
                    "graphql_schema",
                    "line_heuristic",
                    "graphql_field",
                    format!("{parent}.{field_name}"),
                    json!({ "parent_type": parent, "field": field_name }),
                    Some(idx as u32 + 1),
                ));
            }
        }
    }
    facts
}

fn proto_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    let mut facts = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        for service in names_after(line, "service ") {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_service",
                service,
                json!({ "schema": "proto" }),
                Some(idx as u32 + 1),
            ));
        }
        for rpc in rpc_specs(line) {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_rpc",
                rpc.name,
                json!({ "request": rpc.request, "response": rpc.response }),
                Some(idx as u32 + 1),
            ));
        }
        for message in names_after(line, "message ") {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_message",
                message,
                json!({ "schema": "proto" }),
                Some(idx as u32 + 1),
            ));
        }
    }
    facts
}

struct RpcSpec {
    name: String,
    request: String,
    response: String,
}

fn names_after(line: &str, marker: &str) -> Vec<String> {
    line.match_indices(marker)
        .filter_map(|(idx, _)| {
            let rest = &line[idx + marker.len()..];
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

fn rpc_specs(line: &str) -> Vec<RpcSpec> {
    line.match_indices("rpc ")
        .filter_map(|(idx, _)| {
            let rest = &line[idx + 4..];
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            let request = rest.split('(').nth(1)?.split(')').next()?.trim();
            let response = rest
                .split("returns")
                .nth(1)?
                .split('(')
                .nth(1)?
                .split(')')
                .next()?
                .trim();
            Some(RpcSpec {
                name,
                request: request.to_string(),
                response: response.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;

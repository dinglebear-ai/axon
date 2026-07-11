use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::{Value, json};

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "schema";

const HTTP_METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

pub fn api_schema_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let path = input.document.path.as_deref().unwrap_or_default();
    if path.ends_with(".graphql") || inline_text(input).contains("type Query") {
        graphql_facts(input)
    } else if path.ends_with(".proto") || inline_text(input).contains("service ") {
        proto_facts(input)
    } else {
        openapi_facts(input)
    }
}

/// Line/indentation heuristic OpenAPI walker. Extracts endpoints (path +
/// method), `operationId` operations, `components.schemas` definitions, and
/// auth requirements from both `components.securitySchemes` and `security:`
/// requirement lists — the full "endpoints, methods, schemas, operations,
/// auth requirements" fact set required by the API Schema parser family row
/// in the parsing contract. Regex/line-based, not a real YAML AST, so
/// `parser_method` stays "yaml_heuristic" (confidence 0.7).
fn openapi_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_endpoint: Option<String> = None;
    let mut stack: Vec<(usize, String)> = Vec::new();

    for (idx, line) in inline_text(input).lines().enumerate() {
        let line_no = idx as u32 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        while stack.last().is_some_and(|(depth, _)| *depth >= indent) {
            stack.pop();
        }

        if trimmed.starts_with('/') && trimmed.ends_with(':') {
            current_path = Some(trimmed.trim_end_matches(':').to_string());
            current_endpoint = None;
            stack.push((indent, "path".to_string()));
            continue;
        }

        if let Some(method) = trimmed
            .strip_suffix(':')
            .filter(|method| HTTP_METHODS.contains(method))
        {
            if let Some(path) = current_path.as_deref() {
                let name = format!("{} {path}", method.to_ascii_uppercase());
                push_item(
                    input,
                    &mut facts,
                    &mut candidates,
                    "api_endpoint",
                    &name,
                    json!({ "method": method, "path": path }),
                    line_no,
                );
                current_endpoint = Some(name);
            }
            stack.push((indent, "method".to_string()));
            continue;
        }

        if let Some(raw) = trimmed.strip_prefix("operationId:") {
            push_operation(
                input,
                &mut facts,
                &mut candidates,
                current_endpoint.as_deref(),
                raw,
                line_no,
            );
            continue;
        }

        if let Some(raw_scheme) = trimmed
            .strip_prefix("- ")
            .filter(|_| stack_ends_with(&stack, &["security"]))
        {
            push_auth_requirement(input, &mut facts, &mut candidates, raw_scheme, line_no);
            continue;
        }

        if let Some(name) = trimmed.strip_suffix(':') {
            if stack_ends_with(&stack, &["components", "schemas"]) {
                push_item(
                    input,
                    &mut facts,
                    &mut candidates,
                    "api_schema",
                    name,
                    json!({ "schema_name": name }),
                    line_no,
                );
            } else if stack_ends_with(&stack, &["components", "securityschemes"]) {
                push_auth_requirement(input, &mut facts, &mut candidates, name, line_no);
            }
            stack.push((indent, name.to_ascii_lowercase()));
        }
    }
    (facts, candidates)
}

fn push_item(
    input: &ParseInput,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
    fact_kind: &str,
    name: &str,
    value: Value,
    line_no: u32,
) {
    facts.push(source_fact(
        input,
        "openapi_schema",
        "yaml_heuristic",
        fact_kind,
        name,
        value,
        Some(line_no),
    ));
    candidates.push(graph_candidate(
        input,
        "openapi_schema",
        fact_kind,
        name,
        Some(line_no),
        Some(name.to_string()),
    ));
}

fn push_operation(
    input: &ParseInput,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
    endpoint: Option<&str>,
    raw: &str,
    line_no: u32,
) {
    let op_id = raw.trim().trim_matches('"').trim_matches('\'');
    if op_id.is_empty() {
        return;
    }
    let name = match endpoint {
        Some(endpoint) => format!("{endpoint} #{op_id}"),
        None => op_id.to_string(),
    };
    push_item(
        input,
        facts,
        candidates,
        "api_operation",
        &name,
        json!({ "operation_id": op_id, "endpoint": endpoint }),
        line_no,
    );
}

fn push_auth_requirement(
    input: &ParseInput,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
    raw_scheme: &str,
    line_no: u32,
) {
    let scheme = raw_scheme.split(':').next().unwrap_or(raw_scheme).trim();
    if scheme.is_empty() {
        return;
    }
    push_item(
        input,
        facts,
        candidates,
        "api_auth_requirement",
        scheme,
        json!({ "scheme_name": scheme }),
        line_no,
    );
}

fn stack_ends_with(stack: &[(usize, String)], segments: &[&str]) -> bool {
    if stack.len() < segments.len() {
        return false;
    }
    stack[stack.len() - segments.len()..]
        .iter()
        .map(|(_, key)| key.as_str())
        .eq(segments.iter().copied())
}

fn graphql_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let mut current_type: Option<String> = None;
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        let line_number = idx as u32 + 1;
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
                    Some(line_number),
                ));
                candidates.push(graph_candidate(
                    input,
                    "graphql_schema",
                    "graphql_type",
                    name,
                    Some(line_number),
                    Some(trimmed.to_string()),
                ));
            }
        } else if let Some(parent) = current_type.as_deref() {
            if trimmed == "}" {
                current_type = None;
                continue;
            }
            if let Some((field, _)) = trimmed.split_once(':') {
                let field_name = field.split('(').next().unwrap_or(field).trim();
                let name = format!("{parent}.{field_name}");
                facts.push(source_fact(
                    input,
                    "graphql_schema",
                    "line_heuristic",
                    "graphql_field",
                    name.clone(),
                    json!({ "parent_type": parent, "field": field_name }),
                    Some(line_number),
                ));
                candidates.push(graph_candidate(
                    input,
                    "graphql_schema",
                    "graphql_field",
                    &name,
                    Some(line_number),
                    Some(trimmed.to_string()),
                ));
            }
        }
    }
    (facts, candidates)
}

fn proto_facts(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let line_number = idx as u32 + 1;
        for service in names_after(line, "service ") {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_service",
                service.clone(),
                json!({ "schema": "proto" }),
                Some(line_number),
            ));
            candidates.push(graph_candidate(
                input,
                "proto_schema",
                "proto_service",
                &service,
                Some(line_number),
                Some(line.trim().to_string()),
            ));
        }
        for rpc in rpc_specs(line) {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_rpc",
                rpc.name.clone(),
                json!({ "request": rpc.request, "response": rpc.response }),
                Some(line_number),
            ));
            candidates.push(graph_candidate(
                input,
                "proto_schema",
                "proto_rpc",
                &rpc.name,
                Some(line_number),
                Some(line.trim().to_string()),
            ));
        }
        for message in names_after(line, "message ") {
            facts.push(source_fact(
                input,
                "proto_schema",
                "line_heuristic",
                "proto_message",
                message.clone(),
                json!({ "schema": "proto" }),
                Some(line_number),
            ));
            candidates.push(graph_candidate(
                input,
                "proto_schema",
                "proto_message",
                &message,
                Some(line_number),
                Some(line.trim().to_string()),
            ));
        }
    }
    (facts, candidates)
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

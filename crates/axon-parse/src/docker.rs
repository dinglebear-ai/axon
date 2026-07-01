use axon_api::source::SourceParseFacts;
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "docker";

pub fn docker_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    let path = input.document.path.as_deref().unwrap_or_default();
    if path.eq_ignore_ascii_case("Dockerfile") || path.ends_with("/Dockerfile") {
        dockerfile_facts(input)
    } else {
        compose_facts(input)
    }
}

fn dockerfile_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    let mut facts = Vec::new();
    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if let Some(image) = trimmed.strip_prefix("FROM ") {
            facts.push(source_fact(
                input,
                "dockerfile",
                "line_heuristic",
                "docker_base_image",
                image.split_whitespace().next().unwrap_or(image),
                json!({ "image": image.split_whitespace().next().unwrap_or(image) }),
                Some(idx as u32 + 1),
            ));
        } else if let Some(env) = trimmed.strip_prefix("ENV ") {
            let key = env.split(['=', ' ']).next().unwrap_or(env);
            facts.push(source_fact(
                input,
                "dockerfile",
                "line_heuristic",
                "docker_env",
                key,
                json!({ "key": key }),
                Some(idx as u32 + 1),
            ));
        } else if let Some(port) = trimmed.strip_prefix("EXPOSE ") {
            let port = port.split_whitespace().next().unwrap_or(port);
            facts.push(source_fact(
                input,
                "dockerfile",
                "line_heuristic",
                "docker_expose",
                port,
                json!({ "port": port }),
                Some(idx as u32 + 1),
            ));
        }
    }
    facts
}

fn compose_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    let mut facts = Vec::new();
    let mut in_services = false;
    let mut current_service: Option<String> = None;

    for (idx, line) in inline_text(input).lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "services:" {
            in_services = true;
            continue;
        }
        if !in_services {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") && trimmed.ends_with(':') {
            let service = trimmed.trim_end_matches(':').to_string();
            current_service = Some(service.clone());
            facts.push(source_fact(
                input,
                "docker_compose",
                "line_heuristic",
                "compose_service",
                service,
                json!({ "kind": "service" }),
                Some(idx as u32 + 1),
            ));
        } else if let Some(image) = trimmed.strip_prefix("image: ") {
            let name = current_service.as_deref().unwrap_or(image);
            facts.push(source_fact(
                input,
                "docker_compose",
                "line_heuristic",
                "compose_image",
                name,
                json!({ "image": image }),
                Some(idx as u32 + 1),
            ));
        } else if trimmed.starts_with("- ") && current_service.is_some() {
            let port = trimmed.trim_start_matches("- ").trim_matches('"');
            facts.push(source_fact(
                input,
                "docker_compose",
                "line_heuristic",
                "compose_port",
                current_service.as_deref().unwrap_or("service"),
                json!({ "port": port }),
                Some(idx as u32 + 1),
            ));
        }
    }
    facts
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

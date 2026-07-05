use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::candidate_edge;
use crate::parser::ParseInput;

mod compose;

pub const MODULE_NAME: &str = "docker";

const MAX_DOCKER_FILE_BYTES: usize = 512 * 1024;
const MAX_GRAPH_CANDIDATES_PER_DOCUMENT: usize = 2_000;

pub fn docker_parse_items(input: &ParseInput) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let text = inline_text(input);
    if text.len() > MAX_DOCKER_FILE_BYTES {
        return (Vec::new(), Vec::new());
    }

    let path = input.document.path.as_deref().unwrap_or_default();
    if is_dockerfile(path) {
        dockerfile_parse_items(input, text)
    } else {
        compose::compose_parse_items(input, text)
    }
}

pub fn docker_facts(input: &ParseInput) -> Vec<SourceParseFacts> {
    docker_parse_items(input).0
}

fn dockerfile_parse_items(
    input: &ParseInput,
    text: &str,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let mut facts = Vec::new();
    let mut candidates = Vec::new();

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx as u32 + 1;
        let trimmed = line.trim();
        if let Some(image) = trimmed.strip_prefix("FROM ") {
            let image = image.split_whitespace().next().unwrap_or(image);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "dockerfile",
                "docker_base_image",
                image,
                json!({ "docker_image": image }),
                Some(line_no),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "container_manifest",
                    "local_checkout",
                    &local_checkout_key(input),
                    "container_image_tag",
                    &format!("docker:{image}"),
                    "repo_uses_container_image",
                    "container_manifest",
                    Some(line_no),
                    Some(line.to_string()),
                ),
            );
        } else if let Some(env) = trimmed.strip_prefix("ENV ") {
            let key = env.split(['=', ' ']).next().unwrap_or(env);
            let secret = is_secret_key(key);
            let fact_kind = if secret {
                "secret_reference"
            } else {
                "environment_variable"
            };
            facts.push(source_fact(
                input,
                "docker_manifest",
                "dockerfile",
                fact_kind,
                key,
                json!({ "key": key, "value_redacted": true }),
                Some(line_no),
            ));
            push_candidate(
                &mut candidates,
                env_candidate(input, key, secret, line_no, format!("ENV {key}=<redacted>")),
            );
        } else if let Some(port) = trimmed.strip_prefix("EXPOSE ") {
            let port = port.split_whitespace().next().unwrap_or(port);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "dockerfile",
                "network_endpoint",
                port,
                json!({ "docker_port": port }),
                Some(line_no),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "runtime_manifest",
                    "local_checkout",
                    &local_checkout_key(input),
                    "network_endpoint",
                    &format!("endpoint:{}:{port}", repo_key(input)),
                    "service_exposes_endpoint",
                    "runtime_manifest",
                    Some(line_no),
                    Some(line.to_string()),
                ),
            );
        }
    }

    (facts, candidates)
}

fn env_candidate(
    input: &ParseInput,
    key: &str,
    secret: bool,
    line_no: u32,
    quote: String,
) -> GraphCandidate {
    candidate_edge(
        input,
        "docker_manifest",
        "runtime_manifest",
        "local_checkout",
        &local_checkout_key(input),
        if secret {
            "secret_reference"
        } else {
            "environment_variable"
        },
        &format!("{}:{key}", if secret { "secret" } else { "env" }),
        "repo_declares_env_var",
        "runtime_manifest",
        Some(line_no),
        Some(quote),
    )
}

fn is_dockerfile(path: &str) -> bool {
    path.eq_ignore_ascii_case("Dockerfile")
        || path.ends_with("/Dockerfile")
        || path.eq_ignore_ascii_case("Containerfile")
        || path.ends_with("/Containerfile")
}

pub(super) fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "secret",
        "password",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "database_url",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub(super) fn local_checkout_key(input: &ParseInput) -> String {
    format!("local://{}", input.document.source_id.0)
}

pub(super) fn repo_key(input: &ParseInput) -> String {
    input.document.source_id.0.clone()
}

pub(super) fn service_key_for(input: &ParseInput, service: &str) -> String {
    format!("service:{}:{service}", repo_key(input))
}

pub(super) fn push_candidate(candidates: &mut Vec<GraphCandidate>, candidate: GraphCandidate) {
    if candidates.len() < MAX_GRAPH_CANDIDATES_PER_DOCUMENT {
        candidates.push(candidate);
    }
}

pub(super) fn line_for_token(text: &str, token: &str) -> u32 {
    text.lines()
        .position(|line| line.contains(token))
        .map(|idx| idx as u32 + 1)
        .unwrap_or(1)
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;
use serde_yaml::Value as YamlValue;

use crate::facts::{inline_text, source_fact};
use crate::graph_candidate::candidate_edge;
use crate::parser::ParseInput;

pub const MODULE_NAME: &str = "docker";

const MAX_DOCKER_FILE_BYTES: usize = 512 * 1024;
const MAX_COMPOSE_DEPTH: usize = 32;
const MAX_COMPOSE_SERVICES: usize = 256;
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
        compose_parse_items(input, text)
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

fn compose_parse_items(
    input: &ParseInput,
    text: &str,
) -> (Vec<SourceParseFacts>, Vec<GraphCandidate>) {
    let Ok(root) = serde_yaml::from_str::<YamlValue>(text) else {
        return (Vec::new(), Vec::new());
    };
    if yaml_depth(&root) > MAX_COMPOSE_DEPTH {
        return (Vec::new(), Vec::new());
    }

    let mut facts = Vec::new();
    let mut candidates = Vec::new();
    let Some(services) = mapping_get(&root, "services").and_then(YamlValue::as_mapping) else {
        return (facts, candidates);
    };

    for (idx, (service_key, service_value)) in services.iter().enumerate() {
        if idx >= MAX_COMPOSE_SERVICES {
            break;
        }
        let Some(service) = service_key.as_str() else {
            continue;
        };
        let line_no = line_for_token(text, &format!("{service}:"));
        facts.push(source_fact(
            input,
            "docker_manifest",
            "compose",
            "runtime_service",
            service,
            json!({ "docker_service": service }),
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
                "runtime_service",
                &service_key_for(input, service),
                "repo_declares_service",
                "runtime_manifest",
                Some(line_no),
                Some(format!("{service}:")),
            ),
        );

        let Some(service_map) = service_value.as_mapping() else {
            continue;
        };
        if let Some(image) = map_get(service_map, "image").and_then(YamlValue::as_str) {
            let image_line = line_for_token(text, image);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "compose",
                "container_image_tag",
                image,
                json!({ "docker_image": image, "docker_service": service }),
                Some(image_line),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "runtime_manifest",
                    "runtime_service",
                    &service_key_for(input, service),
                    "container_image_tag",
                    &format!("docker:{image}"),
                    "service_uses_image",
                    "runtime_manifest",
                    Some(image_line),
                    Some(format!("image: {image}")),
                ),
            );
        }

        for port in string_values(map_get(service_map, "ports")) {
            let port_line = line_for_token(text, &port);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "compose",
                "network_endpoint",
                port.clone(),
                json!({ "docker_port": port, "docker_service": service }),
                Some(port_line),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "runtime_manifest",
                    "runtime_service",
                    &service_key_for(input, service),
                    "network_endpoint",
                    &format!("endpoint:{}:{service}:{port}", repo_key(input)),
                    "service_exposes_endpoint",
                    "runtime_manifest",
                    Some(port_line),
                    Some(port),
                ),
            );
        }

        for volume in string_values(map_get(service_map, "volumes")) {
            let volume_line = line_for_token(text, &volume);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "compose",
                "volume_mount",
                volume.clone(),
                json!({ "docker_volume": volume, "docker_service": service }),
                Some(volume_line),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "runtime_manifest",
                    "runtime_service",
                    &service_key_for(input, service),
                    "volume_mount",
                    &format!("volume:{}:{service}:{volume}", repo_key(input)),
                    "service_mounts_volume",
                    "runtime_manifest",
                    Some(volume_line),
                    Some(volume),
                ),
            );
        }

        for key in environment_keys(map_get(service_map, "environment")) {
            let secret = is_secret_key(&key);
            let fact_kind = if secret {
                "secret_reference"
            } else {
                "environment_variable"
            };
            let key_line = line_for_token(text, &key);
            facts.push(source_fact(
                input,
                "docker_manifest",
                "compose",
                fact_kind,
                key.clone(),
                json!({ "key": key, "docker_service": service, "value_redacted": true }),
                Some(key_line),
            ));
            push_candidate(
                &mut candidates,
                candidate_edge(
                    input,
                    "docker_manifest",
                    "runtime_manifest",
                    "runtime_service",
                    &service_key_for(input, service),
                    if secret {
                        "secret_reference"
                    } else {
                        "environment_variable"
                    },
                    &format!("{}:{key}", if secret { "secret" } else { "env" }),
                    "service_requires_env",
                    "runtime_manifest",
                    Some(key_line),
                    Some(format!("{key}: <redacted>")),
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

fn is_secret_key(key: &str) -> bool {
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

fn local_checkout_key(input: &ParseInput) -> String {
    format!("local://{}", input.document.source_id.0)
}

fn repo_key(input: &ParseInput) -> String {
    input.document.source_id.0.clone()
}

fn service_key_for(input: &ParseInput, service: &str) -> String {
    format!("service:{}:{service}", repo_key(input))
}

fn push_candidate(candidates: &mut Vec<GraphCandidate>, candidate: GraphCandidate) {
    if candidates.len() < MAX_GRAPH_CANDIDATES_PER_DOCUMENT {
        candidates.push(candidate);
    }
}

fn mapping_get<'a>(value: &'a YamlValue, key: &str) -> Option<&'a YamlValue> {
    value.as_mapping()?.get(YamlValue::String(key.to_string()))
}

fn map_get<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a YamlValue> {
    map.get(YamlValue::String(key.to_string()))
}

fn string_values(value: Option<&YamlValue>) -> Vec<String> {
    match value {
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(YamlValue::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn environment_keys(value: Option<&YamlValue>) -> Vec<String> {
    match value {
        Some(YamlValue::Mapping(map)) => map
            .keys()
            .filter_map(|key| key.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(|value| value.as_str())
            .filter_map(|entry| entry.split_once('=').map(|(key, _)| key).or(Some(entry)))
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn yaml_depth(value: &YamlValue) -> usize {
    match value {
        YamlValue::Sequence(values) => 1 + values.iter().map(yaml_depth).max().unwrap_or(0),
        YamlValue::Mapping(map) => 1 + map.values().map(yaml_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn line_for_token(text: &str, token: &str) -> u32 {
    text.lines()
        .position(|line| line.contains(token))
        .map(|idx| idx as u32 + 1)
        .unwrap_or(1)
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

use axon_api::source::{GraphCandidate, SourceParseFacts};
use serde_json::json;
use serde_yaml::Value as YamlValue;

use crate::facts::source_fact;
use crate::graph_candidate::candidate_edge;
use crate::parser::ParseInput;

const MAX_COMPOSE_DEPTH: usize = 32;
const MAX_COMPOSE_SERVICES: usize = 256;

pub(super) fn compose_parse_items(
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
        parse_service(
            input,
            text,
            service,
            service_value,
            &mut facts,
            &mut candidates,
        );
    }

    (facts, candidates)
}

fn parse_service(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_value: &YamlValue,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    let line_no = super::line_for_token(text, &format!("{service}:"));
    facts.push(source_fact(
        input,
        "docker_manifest",
        "compose",
        "runtime_service",
        service,
        json!({ "docker_service": service }),
        Some(line_no),
    ));
    super::push_candidate(
        candidates,
        candidate_edge(
            input,
            "docker_manifest",
            "runtime_manifest",
            "local_checkout",
            &super::local_checkout_key(input),
            "runtime_service",
            &super::service_key_for(input, service),
            "repo_declares_service",
            "runtime_manifest",
            Some(line_no),
            Some(format!("{service}:")),
        ),
    );

    let Some(service_map) = service_value.as_mapping() else {
        return;
    };
    parse_image(input, text, service, service_map, facts, candidates);
    parse_string_list(
        input,
        text,
        service,
        service_map,
        FieldListSpec {
            yaml_key: "ports",
            fact_kind: "network_endpoint",
            value_key: "docker_port",
            node_kind: "network_endpoint",
            node_prefix: "endpoint",
            edge_kind: "service_exposes_endpoint",
        },
        facts,
        candidates,
    );
    parse_string_list(
        input,
        text,
        service,
        service_map,
        FieldListSpec {
            yaml_key: "volumes",
            fact_kind: "volume_mount",
            value_key: "docker_volume",
            node_kind: "volume_mount",
            node_prefix: "volume",
            edge_kind: "service_mounts_volume",
        },
        facts,
        candidates,
    );
    parse_environment(input, text, service, service_map, facts, candidates);
    parse_env_files(input, text, service, service_map, facts, candidates);
    parse_secrets(input, text, service, service_map, facts, candidates);
    parse_dependencies(input, text, service, service_map, facts, candidates);
}

fn parse_image(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    let Some(image) = map_get(service_map, "image").and_then(YamlValue::as_str) else {
        return;
    };
    let image_line = super::line_for_token(text, image);
    facts.push(source_fact(
        input,
        "docker_manifest",
        "compose",
        "container_image_tag",
        image,
        json!({ "docker_image": image, "docker_service": service }),
        Some(image_line),
    ));
    super::push_candidate(
        candidates,
        candidate_edge(
            input,
            "docker_manifest",
            "runtime_manifest",
            "runtime_service",
            &super::service_key_for(input, service),
            "container_image_tag",
            &format!("docker:{image}"),
            "service_uses_image",
            "runtime_manifest",
            Some(image_line),
            Some(format!("image: {image}")),
        ),
    );
}

struct FieldListSpec {
    yaml_key: &'static str,
    fact_kind: &'static str,
    value_key: &'static str,
    node_kind: &'static str,
    node_prefix: &'static str,
    edge_kind: &'static str,
}

fn parse_string_list(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    spec: FieldListSpec,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    for value in string_values(map_get(service_map, spec.yaml_key)) {
        let line = super::line_for_token(text, &value);
        facts.push(source_fact(
            input,
            "docker_manifest",
            "compose",
            spec.fact_kind,
            value.clone(),
            json!({ spec.value_key: value, "docker_service": service }),
            Some(line),
        ));
        super::push_candidate(
            candidates,
            candidate_edge(
                input,
                "docker_manifest",
                "runtime_manifest",
                "runtime_service",
                &super::service_key_for(input, service),
                spec.node_kind,
                &format!(
                    "{}:{}:{service}:{value}",
                    spec.node_prefix,
                    super::repo_key(input)
                ),
                spec.edge_kind,
                "runtime_manifest",
                Some(line),
                Some(value),
            ),
        );
    }
}

fn parse_environment(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    for key in environment_keys(map_get(service_map, "environment")) {
        push_env_contract(input, text, service, key, facts, candidates);
    }
}

fn push_env_contract(
    input: &ParseInput,
    text: &str,
    service: &str,
    key: String,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    let secret = super::is_secret_key(&key);
    let fact_kind = if secret {
        "secret_reference"
    } else {
        "environment_variable"
    };
    let key_line = super::line_for_token(text, &key);
    facts.push(source_fact(
        input,
        "docker_manifest",
        "compose",
        fact_kind,
        key.clone(),
        json!({ "key": key, "docker_service": service, "value_redacted": true }),
        Some(key_line),
    ));
    super::push_candidate(
        candidates,
        candidate_edge(
            input,
            "docker_manifest",
            "runtime_manifest",
            "runtime_service",
            &super::service_key_for(input, service),
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

fn parse_env_files(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    for env_file in string_values(map_get(service_map, "env_file")) {
        let line = super::line_for_token(text, &env_file);
        facts.push(source_fact(
            input,
            "docker_manifest",
            "compose",
            "env_file",
            env_file.clone(),
            json!({ "env_file": env_file, "docker_service": service }),
            Some(line),
        ));
        super::push_candidate(
            candidates,
            candidate_edge(
                input,
                "docker_manifest",
                "runtime_manifest",
                "runtime_service",
                &super::service_key_for(input, service),
                "artifact",
                &format!("artifact:env_file:{}:{env_file}", super::repo_key(input)),
                "source_produced_artifact",
                "runtime_manifest",
                Some(line),
                Some(env_file),
            ),
        );
    }
}

fn parse_secrets(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    for secret in service_secret_names(map_get(service_map, "secrets")) {
        push_env_contract(input, text, service, secret, facts, candidates);
    }
}

fn parse_dependencies(
    input: &ParseInput,
    text: &str,
    service: &str,
    service_map: &serde_yaml::Mapping,
    facts: &mut Vec<SourceParseFacts>,
    candidates: &mut Vec<GraphCandidate>,
) {
    for dependency in service_dependency_names(map_get(service_map, "depends_on")) {
        let line = super::line_for_token(text, &dependency);
        facts.push(source_fact(
            input,
            "docker_manifest",
            "compose",
            "service_dependency",
            dependency.clone(),
            json!({ "docker_service": service, "depends_on": dependency }),
            Some(line),
        ));
        super::push_candidate(
            candidates,
            candidate_edge(
                input,
                "docker_manifest",
                "runtime_manifest",
                "runtime_service",
                &super::service_key_for(input, service),
                "runtime_service",
                &super::service_key_for(input, &dependency),
                "derived_from",
                "runtime_manifest",
                Some(line),
                Some(dependency),
            ),
        );
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

fn service_secret_names(value: Option<&YamlValue>) -> Vec<String> {
    match value {
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(|value| match value {
                YamlValue::String(secret) => Some(secret.clone()),
                YamlValue::Mapping(map) => map
                    .get(YamlValue::String("source".to_string()))
                    .or_else(|| map.get(YamlValue::String("target".to_string())))
                    .and_then(YamlValue::as_str)
                    .map(ToOwned::to_owned),
                _ => None,
            })
            .collect(),
        Some(YamlValue::Mapping(map)) => map
            .keys()
            .filter_map(|key| key.as_str().map(ToOwned::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn service_dependency_names(value: Option<&YamlValue>) -> Vec<String> {
    match value {
        Some(YamlValue::Sequence(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(YamlValue::Mapping(map)) => map
            .keys()
            .filter_map(|key| key.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(YamlValue::String(value)) => vec![value.clone()],
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

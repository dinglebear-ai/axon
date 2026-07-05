use axon_api::source::*;
use uuid::Uuid;

use crate::docker::{docker_facts, docker_parse_items};
use crate::parser::ParseInput;

fn input(path: &str, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(21)),
        stage_id: StageId::new(Uuid::from_u128(22)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_docker"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: ContentKind::PlainText,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

fn parse_fixture(path: &str, text: &str) -> crate::parser::ParseResult {
    let input = input(path, text);
    let (facts, graph_candidates) = docker_parse_items(&input);
    crate::parser::ParseResult {
        header: crate::parser::stage_header(&input, LifecycleStatus::Completed, Vec::new(), None),
        document_id: input.document.document_id.clone(),
        facts,
        graph_candidates,
        parser_id: "docker_manifest".to_string(),
        parser_version: crate::facts::PARSER_VERSION.to_string(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

fn has_fact(result: &crate::parser::ParseResult, fact_kind: &str, name: &str) -> bool {
    result
        .facts
        .iter()
        .any(|fact| fact.fact_kind == fact_kind && fact.name == name)
}

#[test]
fn dockerfile_parser_emits_image_endpoint_env_and_graph_candidates() {
    let result = parse_fixture(
        "Dockerfile",
        "FROM qdrant/qdrant:v1.13.1\nENV QDRANT__SERVICE__API_KEY=\nEXPOSE 6333\n",
    );

    assert!(has_fact(
        &result,
        "docker_base_image",
        "qdrant/qdrant:v1.13.1"
    ));
    assert!(has_fact(
        &result,
        "secret_reference",
        "QDRANT__SERVICE__API_KEY"
    ));
    assert!(has_fact(&result, "network_endpoint", "6333"));
    assert!(result.graph_candidates.iter().any(|candidate| {
        candidate
            .nodes
            .iter()
            .any(|node| node.node_kind == "container_image_tag")
    }));
    assert!(!result.graph_candidates.is_empty());
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn compose_parser_emits_service_image_port_volume_and_env_graph_candidates() {
    let result = parse_fixture(
        "docker-compose.yml",
        r#"
services:
  qdrant:
    image: qdrant/qdrant:v1.13.1
    ports:
      - "6333:6333"
    volumes:
      - qdrant-data:/qdrant/storage
    environment:
      QDRANT__SERVICE__API_KEY:
volumes:
  qdrant-data:
"#,
    );

    assert!(has_fact(&result, "runtime_service", "qdrant"));
    assert!(has_fact(
        &result,
        "container_image_tag",
        "qdrant/qdrant:v1.13.1"
    ));
    assert!(has_fact(&result, "network_endpoint", "6333:6333"));
    assert!(has_fact(
        &result,
        "volume_mount",
        "qdrant-data:/qdrant/storage"
    ));
    assert!(has_fact(
        &result,
        "secret_reference",
        "QDRANT__SERVICE__API_KEY"
    ));
    assert!(result.graph_candidates.iter().any(|candidate| {
        candidate
            .edges
            .iter()
            .any(|edge| edge.edge_kind == "service_uses_image")
    }));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn compose_parser_emits_env_file_secrets_and_dependencies() {
    let result = parse_fixture(
        "docker-compose.yml",
        r#"
services:
  api:
    image: ghcr.io/acme/api:latest
    env_file:
      - .env
    secrets:
      - db_password
    depends_on:
      db:
        condition: service_healthy
  db:
    image: postgres:16
"#,
    );

    assert!(has_fact(&result, "env_file", ".env"));
    assert!(has_fact(&result, "secret_reference", "db_password"));
    assert!(has_fact(&result, "service_dependency", "db"));
    assert!(result.graph_candidates.iter().any(|candidate| {
        candidate
            .edges
            .iter()
            .any(|edge| edge.edge_kind == "derived_from")
    }));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn extracts_dockerfile_and_compose_facts() {
    let dockerfile = docker_facts(&input(
        "Dockerfile",
        "FROM rust:1.86\nENV RUST_LOG=info\nEXPOSE 8080\n",
    ));
    assert_eq!(dockerfile[0].name, "rust:1.86");
    assert_eq!(dockerfile[0].fact_kind, "docker_base_image");
    assert_eq!(dockerfile[1].fact_kind, "environment_variable");
    assert_eq!(dockerfile[2].fact_kind, "network_endpoint");
    assert_eq!(dockerfile[2].value["docker_port"], "8080");

    let compose = docker_facts(&input(
        "docker-compose.yaml",
        "services:\n  api:\n    image: ghcr.io/acme/api:latest\n    ports:\n      - \"8080:80\"\n",
    ));
    assert_eq!(compose[0].fact_kind, "runtime_service");
    assert_eq!(compose[0].name, "api");
    assert_eq!(compose[1].fact_kind, "container_image_tag");
    assert_eq!(compose[2].value["docker_port"], "8080:80");
}

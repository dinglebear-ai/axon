use axon_api::source::*;
use uuid::Uuid;

use crate::docker::docker_facts;
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

#[test]
fn extracts_dockerfile_and_compose_facts() {
    let dockerfile = docker_facts(&input(
        "Dockerfile",
        "FROM rust:1.86\nENV RUST_LOG=info\nEXPOSE 8080\n",
    ));
    assert_eq!(dockerfile[0].name, "rust:1.86");
    assert_eq!(dockerfile[0].fact_kind, "docker_base_image");
    assert_eq!(dockerfile[1].fact_kind, "docker_env");
    assert_eq!(dockerfile[2].value["port"], "8080");

    let compose = docker_facts(&input(
        "docker-compose.yaml",
        "services:\n  api:\n    image: ghcr.io/acme/api:latest\n    ports:\n      - \"8080:80\"\n",
    ));
    assert_eq!(compose[0].fact_kind, "compose_service");
    assert_eq!(compose[0].name, "api");
    assert_eq!(compose[1].fact_kind, "compose_image");
    assert_eq!(compose[2].value["port"], "8080:80");
}

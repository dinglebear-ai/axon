use axon_api::source::*;
use uuid::Uuid;

use crate::env::{env_example_facts, env_example_parse_items};
use crate::parser::ParseInput;

fn input(text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(31)),
        stage_id: StageId::new(Uuid::from_u128(32)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_env"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(".env.example"),
            canonical_uri: "file:///repo/.env.example".to_string(),
            content_kind: ContentKind::PlainText,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(".env.example".to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

fn parse_fixture(path: &str, text: &str) -> crate::parser::ParseResult {
    let mut input = input(text);
    input.document.source_item_key = SourceItemKey::from(path);
    input.document.canonical_uri = format!("file:///repo/{path}");
    input.document.path = Some(path.to_string());
    let (facts, graph_candidates) = env_example_parse_items(&input);
    crate::parser::ParseResult {
        header: crate::parser::stage_header(&input, LifecycleStatus::Completed, Vec::new(), None),
        document_id: input.document.document_id.clone(),
        facts,
        graph_candidates,
        parser_id: "env_example".to_string(),
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
fn env_example_parser_never_emits_secret_values() {
    let result = parse_fixture(
        ".env.example",
        "DATABASE_URL=postgres://user:pass@db/app\nOPENAI_API_KEY=sk-proj-secret\nPORT=3000\n",
    );

    assert!(has_fact(&result, "secret_reference", "DATABASE_URL"));
    assert!(has_fact(&result, "secret_reference", "OPENAI_API_KEY"));
    assert!(has_fact(&result, "environment_variable", "PORT"));
    let serialized = serde_json::to_string(&result).expect("serialize parse result");
    assert!(!serialized.contains("sk-proj-secret"));
    assert!(!serialized.contains("user:pass"));
    assert!(!result.graph_candidates.is_empty());
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn extracts_env_example_keys_without_secret_values() {
    let facts = env_example_facts(&input(
        "API_URL=https://example.test\nAPI_TOKEN=\n# ignored\n",
    ));

    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].fact_kind, "environment_variable");
    assert_eq!(facts[0].name, "API_URL");
    assert_eq!(facts[0].value["has_default"], true);
    assert_eq!(facts[1].value["has_default"], false);
    assert!(facts[1].value.get("value").is_none());
}

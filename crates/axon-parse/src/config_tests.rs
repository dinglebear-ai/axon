use axon_api::source::*;
use uuid::Uuid;

use crate::config::{config_facts, config_parse_items};
use crate::parser::ParseInput;

fn input(content_kind: ContentKind, path: &str, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(41)),
        stage_id: StageId::new(Uuid::from_u128(42)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_config"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind,
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

fn parse_fixture(content_kind: ContentKind, path: &str, text: &str) -> crate::parser::ParseResult {
    let input = input(content_kind, path, text);
    let (facts, graph_candidates) = config_parse_items(&input);
    crate::parser::ParseResult {
        header: crate::parser::stage_header(&input, LifecycleStatus::Completed, Vec::new(), None),
        document_id: input.document.document_id.clone(),
        facts,
        graph_candidates,
        parser_id: "config".to_string(),
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
fn json_config_emits_leaf_key_facts_and_graph_candidates() {
    let result = parse_fixture(
        ContentKind::Json,
        "config.json",
        r#"{
            "server": { "port": 8080, "host": "0.0.0.0" },
            "feature_flags": { "beta_enabled": true },
            "log_level": "info"
        }"#,
    );

    assert!(has_fact(&result, "config_key", "server.port"));
    assert!(has_fact(&result, "config_key", "server.host"));
    assert!(has_fact(
        &result,
        "config_key",
        "feature_flags.beta_enabled"
    ));
    assert!(has_fact(&result, "config_key", "log_level"));

    let port_fact = result
        .facts
        .iter()
        .find(|fact| fact.name == "server.port")
        .expect("port fact");
    assert_eq!(port_fact.value["value_kind"], "number");
    assert_eq!(port_fact.value["value"], 8080);
    assert_eq!(port_fact.value["value_redacted"], false);

    assert!(!result.graph_candidates.is_empty());
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn yaml_config_emits_nested_leaf_facts() {
    let result = parse_fixture(
        ContentKind::Yaml,
        "settings.yaml",
        "database:\n  host: db.internal\n  port: 5432\n",
    );

    assert!(has_fact(&result, "config_key", "database.host"));
    assert!(has_fact(&result, "config_key", "database.port"));
}

#[test]
fn toml_config_emits_leaf_facts() {
    let result = parse_fixture(
        ContentKind::Toml,
        "config.toml",
        "[server]\nport = 9090\nname = \"axon\"\n",
    );

    assert!(has_fact(&result, "config_key", "server.port"));
    assert!(has_fact(&result, "config_key", "server.name"));
}

#[test]
fn config_parser_never_emits_secret_looking_values() {
    let result = parse_fixture(
        ContentKind::Json,
        "config.json",
        r#"{
            "database": {
                "url": "postgres://user:pass@db.internal/app",
                "api_key": "sk-proj-secret-value"
            },
            "port": 8080
        }"#,
    );

    assert!(has_fact(&result, "secret_reference", "database.url"));
    assert!(has_fact(&result, "secret_reference", "database.api_key"));
    assert!(has_fact(&result, "config_key", "port"));

    let serialized = serde_json::to_string(&result).expect("serialize parse result");
    assert!(!serialized.contains("pass@db.internal"));
    assert!(!serialized.contains("sk-proj-secret-value"));

    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn malformed_config_returns_no_facts_without_panicking() {
    let json = config_facts(&input(ContentKind::Json, "config.json", "{ not valid json"));
    assert!(json.is_empty());

    let yaml = config_facts(&input(
        ContentKind::Yaml,
        "settings.yaml",
        "not: [valid: yaml",
    ));
    assert!(yaml.is_empty());

    let toml = config_facts(&input(ContentKind::Toml, "config.toml", "not = = valid"));
    assert!(toml.is_empty());
}

#[test]
fn empty_config_returns_no_facts() {
    let facts = config_facts(&input(ContentKind::Json, "config.json", ""));
    assert!(facts.is_empty());

    let empty_object = config_facts(&input(ContentKind::Json, "config.json", "{}"));
    assert!(empty_object.is_empty());
}

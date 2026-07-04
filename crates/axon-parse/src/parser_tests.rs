use axon_api::source::*;
use uuid::Uuid;

use crate::builtins::production_registry;
use crate::parser::{ParseInput, ParserCapability, stage_header};
use crate::registry::ParserRegistry;
use crate::testing::{FakeParser, FakeParserRegistry};

fn source_doc(
    content_kind: ContentKind,
    path: Option<&str>,
    mime_type: Option<&str>,
    text: &str,
) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc_1"),
        source_id: SourceId::from("src_1"),
        source_item_key: SourceItemKey::from(path.unwrap_or("item")),
        canonical_uri: format!("file:///repo/{}", path.unwrap_or("item")),
        content_kind,
        content: ContentRef::InlineText {
            text: text.to_string(),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: path.map(ToOwned::to_owned),
        mime_type: mime_type.map(ToOwned::to_owned),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn input(doc: SourceDocument) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(1)),
        stage_id: StageId::new(Uuid::from_u128(2)),
        document: doc,
        requested_parser: None,
    }
}

fn parser(id: &str) -> FakeParser {
    FakeParser::new(ParserCapability {
        parser_id: id.to_string(),
        parser_version: "test".to_string(),
        content_kinds: Vec::new(),
        mime_types: Vec::new(),
        file_extensions: Vec::new(),
        path_suffixes: Vec::new(),
        sniff_prefixes: Vec::new(),
        priority: 0,
    })
}

#[test]
fn registry_selects_by_hint_content_kind_mime_path_and_sniffing() {
    let explicit = parser("explicit");
    let markdown = parser("markdown").with_content_kind(ContentKind::Markdown);
    let mime = parser("mime_markdown").with_mime_type("text/markdown");
    let manifest = parser("cargo_manifest").with_file_extension("toml");
    let openapi = parser("openapi").with_sniff_prefix("{\"openapi\"");

    let registry = ParserRegistry::new()
        .with_parser(markdown)
        .with_parser(mime)
        .with_parser(manifest)
        .with_parser(openapi)
        .with_parser(explicit);

    let mut explicit_input = input(source_doc(ContentKind::PlainText, None, None, "plain"));
    explicit_input.requested_parser = Some("explicit".to_string());
    assert_eq!(
        registry
            .select(&explicit_input)
            .expect("explicit parser")
            .capability()
            .parser_id,
        "explicit"
    );

    assert_eq!(
        registry
            .select(&input(source_doc(
                ContentKind::Markdown,
                Some("README.md"),
                None,
                "# Readme",
            )))
            .expect("content-kind parser")
            .capability()
            .parser_id,
        "markdown"
    );

    assert_eq!(
        registry
            .select(&input(source_doc(
                ContentKind::PlainText,
                Some("README"),
                Some("text/markdown"),
                "# Readme",
            )))
            .expect("mime parser")
            .capability()
            .parser_id,
        "mime_markdown"
    );

    assert_eq!(
        registry
            .select(&input(source_doc(
                ContentKind::Toml,
                Some("Cargo.toml"),
                None,
                "[dependencies]",
            )))
            .expect("extension parser")
            .capability()
            .parser_id,
        "cargo_manifest"
    );

    assert_eq!(
        registry
            .select(&input(source_doc(
                ContentKind::Json,
                Some("openapi.json"),
                None,
                "{\"openapi\":\"3.1.0\"}",
            )))
            .expect("sniff parser")
            .capability()
            .parser_id,
        "openapi"
    );
}

#[test]
fn registry_specific_matches_beat_generic_content_kind() {
    let generic_markdown = parser("generic_markdown")
        .with_content_kind(ContentKind::Markdown)
        .with_priority(0);
    let readme_markdown = parser("readme_markdown")
        .with_path_suffix("README.md")
        .with_priority(100);
    let registry = ParserRegistry::new()
        .with_parser(generic_markdown)
        .with_parser(readme_markdown);

    let selected = registry
        .select(&input(source_doc(
            ContentKind::Markdown,
            Some("README.md"),
            None,
            "# Readme",
        )))
        .expect("path-specific parser");

    assert_eq!(selected.capability().parser_id, "readme_markdown");
}

#[test]
fn registry_uses_priority_as_same_score_tie_breaker() {
    let low = parser("low_priority")
        .with_file_extension("toml")
        .with_priority(50);
    let high = parser("high_priority")
        .with_file_extension("toml")
        .with_priority(5);
    let registry = ParserRegistry::new().with_parser(low).with_parser(high);

    let selected = registry
        .select(&input(source_doc(
            ContentKind::Toml,
            Some("Cargo.toml"),
            None,
            "[package]",
        )))
        .expect("priority-selected parser");

    assert_eq!(selected.capability().parser_id, "high_priority");
}

#[test]
fn unsupported_input_degrades_to_warning_result() {
    let registry = ParserRegistry::new();
    let result = registry.parse(&input(source_doc(
        ContentKind::BinaryMetadata,
        Some("logo.png"),
        Some("image/png"),
        "",
    )));

    assert_eq!(result.parser_id, "none");
    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, "parse.unsupported");
    assert!(result.facts.is_empty());
    assert!(result.graph_candidates.is_empty());
}

#[test]
fn requested_missing_parser_degrades_without_fallback() {
    let registry = ParserRegistry::new()
        .with_parser(parser("fallback").with_content_kind(ContentKind::Markdown));
    let mut request = input(source_doc(
        ContentKind::Markdown,
        Some("README.md"),
        None,
        "# Readme",
    ));
    request.requested_parser = Some("missing_parser".to_string());

    let result = registry.parse(&request);

    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(result.parser_id, "missing_parser");
    assert_eq!(
        result.warnings[0].code,
        "parse.requested_parser_unavailable"
    );
    assert!(result.facts.is_empty());
}

#[test]
fn production_registry_runs_real_parser_families() {
    let registry = production_registry();

    let manifest = registry.parse(&input(source_doc(
        ContentKind::Json,
        Some("package.json"),
        None,
        r#"{"dependencies":{"vite":"7"}}"#,
    )));
    assert_eq!(manifest.parser_id, "manifest");
    assert_eq!(manifest.facts[0].name, "vite");

    let markdown = registry.parse(&input(source_doc(
        ContentKind::Markdown,
        Some("README.md"),
        None,
        "# Readme",
    )));
    assert_eq!(markdown.parser_id, "markdown_headings");
    assert_eq!(markdown.facts[0].fact_kind, "markdown_heading");

    let docker = registry.parse(&input(source_doc(
        ContentKind::PlainText,
        Some("Dockerfile"),
        None,
        "FROM qdrant/qdrant:v1.13.1\nEXPOSE 6333\n",
    )));
    assert_eq!(docker.parser_id, "docker_manifest");
    assert!(docker.facts.iter().any(|fact| {
        fact.fact_kind == "docker_base_image" && fact.name == "qdrant/qdrant:v1.13.1"
    }));

    let env = registry.parse(&input(source_doc(
        ContentKind::PlainText,
        Some(".env.example"),
        None,
        "QDRANT_URL=http://localhost:6333\nOPENAI_API_KEY=\n",
    )));
    assert_eq!(env.parser_id, "env_example");
    assert!(
        env.facts
            .iter()
            .any(|fact| fact.fact_kind == "env_var" && fact.name == "QDRANT_URL")
    );
    assert!(
        env.facts
            .iter()
            .any(|fact| { fact.fact_kind == "secret_reference" && fact.name == "OPENAI_API_KEY" })
    );
}

#[test]
fn fake_parser_registry_wraps_registry_selection_for_tests() {
    let registry = FakeParserRegistry::new().with_parser(
        parser("fake_markdown")
            .with_content_kind(ContentKind::Markdown)
            .with_fact(SourceParseFacts {
                document_id: DocumentId::from("doc_1"),
                source_item_key: SourceItemKey::from("README.md"),
                fact_kind: "markdown_heading".to_string(),
                name: "Readme".to_string(),
                value: serde_json::json!({ "level": 1 }),
                parser_id: "fake_markdown".to_string(),
                parser_version: "test".to_string(),
                parser_method: "fake".to_string(),
                range: None,
                confidence: 1.0,
                metadata: MetadataMap::new(),
            }),
    );

    let result = registry.parse(&input(source_doc(
        ContentKind::Markdown,
        Some("README.md"),
        None,
        "# Readme",
    )));

    assert_eq!(result.parser_id, "fake_markdown");
    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.facts[0].name, "Readme");
}

#[test]
fn stage_header_uses_runtime_timestamps() {
    let header = stage_header(
        &input(source_doc(
            ContentKind::PlainText,
            Some("notes.txt"),
            None,
            "hi",
        )),
        LifecycleStatus::Completed,
        Vec::new(),
        None,
    );

    assert_ne!(header.started_at.0, "2026-07-01T00:00:00Z");
    assert_eq!(header.completed_at.as_ref(), Some(&header.started_at));
    assert!(header.started_at.0.ends_with("+00:00") || header.started_at.0.ends_with('Z'));
}

#[test]
fn fake_parser_emits_api_facts_and_deterministic_graph_candidate() {
    let fact = SourceParseFacts {
        document_id: DocumentId::from("doc_1"),
        source_item_key: SourceItemKey::from("Cargo.toml"),
        fact_kind: "dependency".to_string(),
        name: "tokio".to_string(),
        value: serde_json::json!({ "version": "1" }),
        parser_id: "cargo_manifest".to_string(),
        parser_version: "test".to_string(),
        parser_method: "toml_parser".to_string(),
        range: None,
        confidence: 1.0,
        metadata: MetadataMap::new(),
    };
    let candidate = GraphCandidate {
        candidate_id: "cand_src_1_Cargo_toml_tokio".to_string(),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::from("src_1"),
        source_item_key: SourceItemKey::from("Cargo.toml"),
        item_canonical_uri: "file:///repo/Cargo.toml".to_string(),
        document_id: Some(DocumentId::from("doc_1")),
        kind: "manifest_dependency".to_string(),
        merge_key: Some("cargo:file:///repo/Cargo.toml:tokio".to_string()),
        producer: GraphCandidateProducer {
            adapter: "local".to_string(),
            parser: Some("cargo_manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 1.0,
        metadata: MetadataMap::new(),
    };
    let registry = ParserRegistry::new().with_parser(
        parser("cargo_manifest")
            .with_file_extension("toml")
            .with_fact(fact.clone())
            .with_graph_candidate(candidate.clone()),
    );

    let result = registry.parse(&input(source_doc(
        ContentKind::Toml,
        Some("Cargo.toml"),
        None,
        "[dependencies]\ntokio = \"1\"",
    )));
    let round_trip: ParseResult =
        serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();

    assert_eq!(round_trip.facts, vec![fact]);
    assert_eq!(round_trip.graph_candidates, vec![candidate]);
    assert_eq!(round_trip.parser_id, "cargo_manifest");
    assert_eq!(round_trip.header.status, LifecycleStatus::Completed);
}

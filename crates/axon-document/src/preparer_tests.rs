use axon_api::source::{
    ChunkHint, ChunkId, ChunkProfile, ContentKind, ContentRef, DocumentId, GraphCandidate,
    GraphCandidateProducer, GraphEvidence, MetadataMap, Severity, SourceDocument, SourceError,
    SourceGenerationId, SourceId, SourceItemKey, SourceParseFacts, SourceRange, SourceWarning,
};
use axon_parse::vertical::{
    VERTICAL_GRAPH_CANDIDATES_METADATA_KEY, VERTICAL_PARSE_FACTS_METADATA_KEY,
};

use crate::{
    ChunkingProfile, DocumentPreparer, DocumentPreparerConfig, PrepareSourceDocumentRequest,
    PrepareSourceDocumentResult,
    preparer::{
        PREPARATION_SCHEMA_VERSION, validate_prepared_document,
        validate_prepared_document_ranges_against_bounds,
    },
    source_range::bounds_for_text,
    testing::RecordingPreparer,
};

#[test]
fn preparation_schema_version_is_semantic_and_stable() {
    assert_eq!(PREPARATION_SCHEMA_VERSION, "axon-document/schema-8");
    assert!(!PREPARATION_SCHEMA_VERSION.contains("pr"));
}

#[test]
fn preparer_uses_injected_markdown_limits_instead_of_ambient_configuration() {
    let preparer = DocumentPreparer::new(DocumentPreparerConfig {
        markdown_max_chars: 96,
        markdown_min_chars: 1,
        markdown_overlap_chars: 0,
        ..DocumentPreparerConfig::default()
    });
    let PrepareSourceDocumentResult::Prepared(prepared) = preparer
        .prepare(request(
            ContentKind::Markdown,
            &format!("# Explicit\n{}", "content ".repeat(40)),
            "gen-explicit-limits",
            ChunkingProfile::MarkdownSections,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert!(prepared.chunks.len() > 1);
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 96)
    );
}

#[test]
fn document_preparation_hot_path_has_no_ambient_config_lookup() {
    let markdown_source = include_str!("markdown.rs");
    let preparer_source = include_str!("preparer.rs");

    assert!(!markdown_source.contains("axon_core::config"));
    assert!(!preparer_source.contains("axon_core::config"));
}

#[test]
fn preparer_builds_prepared_document_from_inline_source_dto() {
    let request = request(
        ContentKind::Markdown,
        "# Intro\nHello to everyone reading this complete application reference.\n\n## Next\nWorld configuration is explained in this complete next section.",
        "gen-1",
        ChunkingProfile::MarkdownSections,
    );

    let result = DocumentPreparer::default().prepare(request).unwrap();
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.document_id, DocumentId::from("doc-test"));
    assert_eq!(prepared.source_id, SourceId::from("src-test"));
    assert_eq!(prepared.source_item_key, SourceItemKey::from("item-test"));
    assert_eq!(prepared.generation, SourceGenerationId::from("gen-1"));
    assert_eq!(prepared.chunking_profile, "markdown_sections");
    assert_eq!(prepared.chunks.len(), 2);
    assert!(
        prepared.chunks[0]
            .chunk_key
            .contains("src-test:gen-1:item-test:markdown_sections")
    );
    assert_eq!(prepared.chunks[0].chunk_index, 0);
    assert_eq!(prepared.chunks[0].source_range.line_start, Some(1));
    assert_eq!(prepared.chunks[0].source_range.byte_start, Some(0));
    assert_eq!(
        prepared.chunks[1].previous_chunk_id,
        Some(prepared.chunks[0].chunk_id.clone())
    );
    assert_eq!(
        prepared.chunks[0].next_chunk_id,
        Some(prepared.chunks[1].chunk_id.clone())
    );
    assert_eq!(
        prepared.chunks[0].metadata["chunking_profile"],
        "markdown_sections"
    );
}

#[test]
fn self_parsed_markdown_overrides_generic_local_code_hint() {
    let mut request = request(
        ContentKind::Markdown,
        "# Observable beacon\n\nProtected values must never enter telemetry.\n",
        "gen-self-parsed-markdown",
        ChunkingProfile::MarkdownSections,
    );
    request.profile = None;
    request.document.chunk_hints.push(ChunkHint {
        profile: ChunkProfile::CodeSymbol,
        reason: "route default chunk profile".to_string(),
        options: MetadataMap::new(),
    });

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request)
        .expect("self-parsed markdown should produce chunks")
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_profile, "markdown_sections");
    assert_eq!(prepared.chunks.len(), 1);
    assert!(prepared.chunks[0].content.contains("Observable beacon"));
}

#[test]
fn large_intact_markdown_preserves_sections_instead_of_paragraph_fallback() {
    let paragraphs = (0..2_000)
        .map(|index| format!("paragraph {index}: {}", "reference text ".repeat(8)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let text = format!("# Large reference\n\n{paragraphs}\n");
    assert!(text.len() > crate::chunk_router::LARGE_DOCUMENT_BYTES);
    let request = request(
        ContentKind::Markdown,
        &text,
        "gen-large-markdown",
        ChunkingProfile::MarkdownSections,
    );

    let PrepareSourceDocumentResult::Prepared(prepared) =
        DocumentPreparer::default().prepare(request).unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_method, "heading_sections");
    assert!(
        prepared.chunks.len() < 500,
        "bounded structural windows should not degrade into one chunk per paragraph"
    );
}

#[test]
fn recording_preparer_records_requests_and_returns_real_prepared_documents() {
    let mut recorder = RecordingPreparer::new(DocumentPreparer::default());
    let request = request(
        ContentKind::PlainText,
        "alpha content with enough useful context for embedding\r\n\r\nbeta content providing a second complete paragraph",
        "gen-fake",
        ChunkingProfile::PlainTextWindows,
    );

    let result = recorder.prepare(request.clone()).unwrap();

    assert_eq!(recorder.requests(), &[request]);
    let PrepareSourceDocumentResult::Prepared(result) = result else {
        panic!("expected prepared document")
    };
    assert_eq!(result.chunking_profile, "plain_text_windows");
    assert_eq!(result.chunks.len(), 1);
}

#[test]
fn preparer_skips_whitespace_only_content() {
    let request = request(
        ContentKind::PlainText,
        " \n\n\t",
        "gen-empty",
        ChunkingProfile::PlainTextWindows,
    );

    assert!(
        matches!(DocumentPreparer::default().prepare(request).unwrap(),
        PrepareSourceDocumentResult::Skipped(skipped) if skipped.reason == axon_api::source::ContentSkipReason::EmptyContent)
    );
}

#[test]
fn preparer_indexes_nonempty_html_when_visible_projection_has_no_chunks() {
    let body = "<script>window.example = \"a complete example value with additional source context\";</script>";
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Html,
            body,
            "gen-empty-html-projection",
            ChunkingProfile::HtmlArticle,
        ))
        .unwrap()
    else {
        panic!("expected source-text fallback")
    };
    assert_eq!(prepared.chunking_method, "plain_text_windows");
    assert_eq!(prepared.chunks.len(), 1);
    assert_eq!(prepared.chunks[0].content, body);
    assert_eq!(prepared.chunks[0].source_range.byte_start, Some(0));
    assert!(
        prepared
            .warnings
            .iter()
            .any(|w| w.code == "chunk.empty_fallback")
    );
}

#[test]
fn validate_prepared_document_rejects_duplicate_chunk_identity() {
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "alpha useful content with enough context to prepare and validate\n\nbeta useful content with another complete paragraph",
            "gen-duplicates",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    let mut invalid = prepared;
    invalid.chunks.push(invalid.chunks[0].clone());
    invalid.chunks[1].chunk_id = invalid.chunks[0].chunk_id.clone();
    invalid.chunks[1].chunk_key = invalid.chunks[0].chunk_key.clone();

    let error = validate_prepared_document(&invalid).unwrap_err();

    assert!(error.contains("duplicate chunk id"));
    assert!(error.contains("duplicate chunk key"));
}

#[test]
fn validate_prepared_document_rejects_impossible_ranges_and_empty_content() {
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "alpha useful content with enough context for range validation",
            "gen-invalid-range",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    let mut invalid = prepared;
    invalid.chunks[0].chunk_id = ChunkId::from("manual-empty");
    invalid.chunks[0].content = " \n\t ".to_string();
    invalid.chunks[0].source_range.byte_start = Some(10);
    invalid.chunks[0].source_range.byte_end = Some(5);
    invalid.chunks[0].chunk_locator.range.line_start = Some(3);
    invalid.chunks[0].chunk_locator.range.line_end = Some(2);

    let error = validate_prepared_document(&invalid).unwrap_err();

    assert!(error.contains("empty content"));
    assert!(error.contains("source_range byte_start > byte_end"));
    assert!(error.contains("locator range line_start > line_end"));
}

#[test]
fn preparer_degrades_chunk_and_parse_fact_ranges_outside_normalized_document() {
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "PORT=3000\nDESCRIPTION=Complete configuration reference for local testing\n",
            "gen-bounds",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    let mut invalid = prepared;
    invalid.chunks[0].source_range.line_start = Some(9000);
    invalid.chunks[0].source_range.line_end = Some(9001);

    let bounds = bounds_for_text(
        "PORT=3000\nDESCRIPTION=Complete configuration reference for local testing\n",
    );
    let err = validate_prepared_document_ranges_against_bounds(
        &invalid,
        &bounds,
        Some("PORT=3000\nDESCRIPTION=Complete configuration reference for local testing\n"),
    )
    .expect_err("range outside normalized document rejected");
    assert!(err.contains("outside normalized document"));
}

#[test]
fn preparer_rejects_graph_evidence_ranges_outside_normalized_document() {
    let source_text = "FROM alpine:3\n# Complete container reference for graph bounds testing\n";
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            source_text,
            "gen-graph-bounds",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    let mut invalid = prepared;
    invalid.graph_candidates.push(GraphCandidate {
        candidate_id: "cand-graph-range".to_string(),
        job_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000001\"").unwrap(),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        item_canonical_uri: "file:///test.md".to_string(),
        document_id: Some(DocumentId::from("doc-test")),
        kind: "container_manifest".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some("docker_manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        evidence: vec![GraphEvidence {
            evidence_id: "ev-out-of-range".to_string(),
            evidence_kind: "container_manifest".to_string(),
            source_id: SourceId::from("src-test"),
            source_item_key: SourceItemKey::from("item-test"),
            document_id: Some(DocumentId::from("doc-test")),
            chunk_id: None,
            range: Some(SourceRange {
                line_start: Some(9000),
                line_end: Some(9000),
                byte_start: None,
                byte_end: None,
                char_start: None,
                char_end: None,
                time_start_ms: None,
                time_end_ms: None,
                dom_selector: None,
                json_pointer: None,
                yaml_path: None,
                xml_xpath: None,
                csv_row: None,
                session_turn_id: None,
                turn_start: None,
                turn_end: None,
            }),
            quote: Some("FROM alpine:3".to_string()),
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    });

    let bounds = bounds_for_text(source_text);
    let err =
        validate_prepared_document_ranges_against_bounds(&invalid, &bounds, Some(source_text))
            .expect_err("graph evidence range outside normalized document rejected");
    assert!(err.contains("graph candidate cand-graph-range evidence ev-out-of-range range"));
}

#[test]
fn preparer_rejects_unordered_time_and_turn_ranges() {
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "first complete line containing useful preparable information\nsecond complete line\n",
            "gen-time-turn",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    let mut invalid = prepared;
    invalid.chunks[0].source_range.time_start_ms = Some(200);
    invalid.chunks[0].source_range.time_end_ms = Some(100);
    invalid.chunks[0].chunk_locator.range.turn_start = Some("turn-9".to_string());
    invalid.chunks[0].chunk_locator.range.turn_end = Some("turn-1".to_string());

    let error = validate_prepared_document(&invalid).unwrap_err();

    assert!(error.contains("source_range time_start_ms > time_end_ms"));
    assert!(error.contains("locator range turn_start > turn_end"));
}

#[test]
fn tool_output_chunks_promote_jsonl_record_metadata() {
    let mut doc = source_doc(
        ContentKind::Structured,
        r#"{"tool":"shell","action":"exec","side_effect_class":"read","output":{"artifact_id":"art_1"}}"#,
    );
    doc.path = Some("tool-output.jsonl".to_string());
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("tool"));

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-tool-output"),
            profile: Some(ChunkingProfile::ToolOutput),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunks.len(), 1);
    let metadata = &prepared.chunks[0].metadata;
    assert_eq!(metadata["segment_kind"], "tool_output");
    assert_eq!(metadata["tool_name"], "shell");
    assert_eq!(metadata["tool_action"], "exec");
    assert_eq!(metadata["tool_side_effect_class"], "read");
    assert_eq!(metadata["tool_output_artifact_id"], "art_1");
}

#[test]
fn preparer_splits_repomix_packed_files_before_code_chunking() {
    let packed = "\
================================================================\n\
File: src/lib.rs\n\
================================================================\n\
pub fn alpha() { let descriptive_value = 42; println!(\"{descriptive_value}\"); }\n\
\n\
================================================================\n\
File: src/main.rs\n\
================================================================\n\
fn main() { let descriptive_value = 42; println!(\"{descriptive_value}\"); }\n";
    let result = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Code,
            packed,
            "gen-repomix",
            ChunkingProfile::CodeSymbol,
        ))
        .unwrap();

    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("expected prepared document")
    };
    let chunks = prepared.chunks;

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].metadata["original_path"], "src/lib.rs");
    assert_eq!(chunks[1].metadata["original_path"], "src/main.rs");
    assert_eq!(chunks[0].chunk_locator.path.as_deref(), Some("src/lib.rs"));
    assert_eq!(chunks[1].chunk_locator.path.as_deref(), Some("src/main.rs"));
    assert!(chunks[0].content.contains("alpha"));
    assert!(chunks[1].content.contains("main"));
}

#[test]
fn preparer_carries_parse_artifacts_to_prepared_document() {
    let fact = SourceParseFacts {
        document_id: DocumentId::from("doc-test"),
        source_item_key: SourceItemKey::from("item-test"),
        fact_kind: "dependency".to_string(),
        name: "tokio".to_string(),
        value: serde_json::json!({ "version": "1" }),
        parser_id: "cargo_manifest".to_string(),
        parser_version: "test".to_string(),
        parser_method: "toml_parser".to_string(),
        range: None,
        confidence: 0.9,
        metadata: MetadataMap::new(),
    };
    let candidate = GraphCandidate {
        candidate_id: "cand-test".to_string(),
        job_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000000\"").unwrap(),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        item_canonical_uri: "file:///test.md".to_string(),
        document_id: Some(DocumentId::from("doc-test")),
        kind: "dependency".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some("cargo_manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 0.9,
        metadata: MetadataMap::new(),
    };
    let warning = SourceWarning {
        code: "parse.warn".to_string(),
        severity: Severity::Warning,
        message: "warn".to_string(),
        source_item_key: Some(SourceItemKey::from("item-test")),
        retryable: false,
    };
    let error = SourceError {
        code: "parse.error".to_string(),
        severity: Severity::Failed,
        message: "error".to_string(),
        source_item_key: Some(SourceItemKey::from("item-test")),
        retryable: false,
        provider_id: None,
        cause: None,
    };

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: source_doc(
                ContentKind::PlainText,
                "A complete useful body with enough context to prepare its parse artifacts.",
            ),
            generation: SourceGenerationId::from("gen-artifacts"),
            profile: Some(ChunkingProfile::PlainTextWindows),
            parse_facts: vec![fact.clone()],
            graph_candidates: vec![candidate.clone()],
            warnings: vec![warning.clone()],
            errors: vec![error.clone()],
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.parse_facts, vec![fact]);
    assert_eq!(prepared.graph_candidates, vec![candidate]);
    assert_eq!(prepared.warnings, vec![warning]);
    assert_eq!(prepared.errors, vec![error]);
}

#[test]
fn preparer_consumes_vertical_parse_artifacts_without_leaking_bridge_metadata() {
    let fact = SourceParseFacts {
        document_id: DocumentId::from("doc-test"),
        source_item_key: SourceItemKey::from("item-test"),
        fact_kind: "repository".to_string(),
        name: "jmagar/axon".to_string(),
        value: serde_json::json!({ "git_provider": "github" }),
        parser_id: "vertical_github_repo".to_string(),
        parser_version: "3".to_string(),
        parser_method: "vertical_metadata".to_string(),
        range: None,
        confidence: 0.95,
        metadata: MetadataMap::new(),
    };
    let candidate = GraphCandidate {
        candidate_id: "cand-vertical".to_string(),
        job_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000000\"").unwrap(),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        item_canonical_uri: "https://github.com/jmagar/axon".to_string(),
        document_id: Some(DocumentId::from("doc-test")),
        kind: "github_repo_metadata".to_string(),
        merge_key: Some("github_repo:github.com/jmagar/axon".to_string()),
        producer: GraphCandidateProducer {
            adapter: "axon-adapters::web::vertical".to_string(),
            parser: Some("vertical_github_repo".to_string()),
            version: "3".to_string(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 0.95,
        metadata: MetadataMap::new(),
    };
    let mut doc = source_doc(
        ContentKind::Markdown,
        "# Axon\n\nRepository metadata describing the project architecture and its source pipelines.",
    );
    doc.metadata.insert(
        VERTICAL_PARSE_FACTS_METADATA_KEY.to_string(),
        serde_json::to_value(vec![fact.clone()]).unwrap(),
    );
    doc.metadata.insert(
        VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(),
        serde_json::to_value(vec![candidate.clone()]).unwrap(),
    );

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-vertical"),
            profile: Some(ChunkingProfile::MarkdownSections),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.parse_facts, vec![fact]);
    assert_eq!(prepared.graph_candidates, vec![candidate]);
    assert!(
        !prepared
            .metadata
            .contains_key(VERTICAL_PARSE_FACTS_METADATA_KEY)
    );
    assert!(
        !prepared
            .metadata
            .contains_key(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY)
    );
    assert!(prepared.chunks.iter().all(|chunk| {
        !chunk
            .metadata
            .contains_key(VERTICAL_PARSE_FACTS_METADATA_KEY)
            && !chunk
                .metadata
                .contains_key(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY)
    }));
}

#[test]
fn malformed_structured_text_degrades_with_fallback_warning() {
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Json,
            "{\"broken\": \"a malformed record containing enough useful text for fallback",
            "gen-structured",
            ChunkingProfile::StructuredRecords,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunks.len(), 1);
    assert_eq!(
        prepared.chunks[0].metadata["chunking_fallback"],
        "atomic_text"
    );
    assert_eq!(prepared.warnings.len(), 1);
    assert_eq!(prepared.warnings[0].code, "chunk.structured_parse_failed");
    assert_eq!(
        prepared.chunks[0].metadata["actual_chunking_method"],
        "atomic_fallback"
    );
}

#[test]
fn unsupported_content_returns_skipped_identity_without_a_document() {
    use axon_api::source::{ArtifactId, ContentSkipReason};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let cases = [
        (
            ContentRef::External {
                uri: "https://user:secret@example.test/body".into(),
                integrity: None,
            },
            ContentSkipReason::UnresolvedContentReference,
        ),
        (
            ContentRef::Artifact {
                artifact_id: ArtifactId("secret-reference".into()),
            },
            ContentSkipReason::UnresolvedContentReference,
        ),
        (
            ContentRef::InlineBytes {
                bytes_base64: STANDARD.encode(b"%PDF-1.7"),
                mime_type: "text/plain".into(),
            },
            ContentSkipReason::UnsupportedBinary,
        ),
        (
            ContentRef::InlineText {
                text: String::new(),
            },
            ContentSkipReason::EmptyContent,
        ),
    ];
    for (content, reason) in cases {
        let mut input = request(
            ContentKind::BinaryMetadata,
            "",
            "gen-skipped",
            ChunkingProfile::AtomicMetadata,
        );
        input.document.content = content;
        let PrepareSourceDocumentResult::Skipped(skipped) =
            DocumentPreparer::default().prepare(input).unwrap()
        else {
            panic!("expected skipped identity, never a placeholder document")
        };
        assert_eq!(skipped.reason, reason);
        assert_eq!(skipped.document_id, DocumentId::from("doc-test"));
        assert_eq!(skipped.source_id, SourceId::from("src-test"));
        assert_eq!(skipped.source_item_key, SourceItemKey::from("item-test"));
        assert_eq!(skipped.generation, SourceGenerationId::from("gen-skipped"));
    }
}

#[test]
fn authored_atomic_metadata_remains_searchable() {
    let input = request(
        ContentKind::BinaryMetadata,
        "%PDF- is a documented signature with complete contextual metadata for retrieval",
        "gen-authored",
        ChunkingProfile::AtomicMetadata,
    );
    let PrepareSourceDocumentResult::Prepared(prepared) =
        DocumentPreparer::default().prepare(input).unwrap()
    else {
        panic!("authored metadata should prepare")
    };
    assert_eq!(prepared.chunking_profile, "atomic_metadata");
    assert_eq!(prepared.chunks.len(), 1);
    assert!(prepared.chunks[0].content.contains("documented signature"));
}

#[test]
fn preparation_enforces_content_ceiling_and_rejects_admitted_malformed_base64() {
    use axon_api::source::ContentSkipReason;
    let preparer = DocumentPreparer::new(DocumentPreparerConfig {
        max_content_bytes: 4,
        ..DocumentPreparerConfig::default()
    });
    let input = request(
        ContentKind::PlainText,
        "large",
        "gen-limit",
        ChunkingProfile::PlainTextWindows,
    );
    assert!(
        matches!(preparer.prepare(input).unwrap(), PrepareSourceDocumentResult::Skipped(s) if s.reason == ContentSkipReason::SizeLimitExceeded)
    );
    let mut input = request(
        ContentKind::PlainText,
        "",
        "gen-error",
        ChunkingProfile::PlainTextWindows,
    );
    input.document.content = ContentRef::InlineBytes {
        bytes_base64: "$$$$".into(),
        mime_type: "secret".into(),
    };
    assert_eq!(
        preparer.prepare(input).unwrap_err(),
        "malformed base64 content"
    );
}

#[test]
fn decoded_utf16_is_redacted_before_parsing_and_range_validation() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let secret = format!("sk-{}", "a".repeat(28));
    let text = format!(
        "# Title\nAuthorization: Bearer {secret}\n\n## After the secret\nBody text provides useful context for retrieval after the credential example.\n"
    );
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    let mut input = request(
        ContentKind::Markdown,
        "",
        "gen-utf16",
        ChunkingProfile::MarkdownSections,
    );
    input.profile = None;
    input.document.content = ContentRef::InlineBytes {
        bytes_base64: STANDARD.encode(bytes),
        mime_type: "application/octet-stream".into(),
    };
    let PrepareSourceDocumentResult::Prepared(prepared) =
        DocumentPreparer::default().prepare(input).unwrap()
    else {
        panic!("UTF16 source text should prepare")
    };
    assert!(!prepared.chunks.is_empty());
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| !chunk.content.contains(&secret))
    );
    assert!(prepared.graph_candidates.iter().any(|candidate| {
        candidate
            .merge_key
            .as_deref()
            .is_some_and(|key| key.contains("After the secret"))
    }));
    assert!(prepared.warnings.iter().any(|warning| {
        warning.code == "document.content.pre_chunk_redacted" && warning.severity == Severity::Info
    }));
}

#[test]
fn large_code_document_dispatches_to_windowed_fallback_not_code_symbols() {
    // Over the 200_000-byte router threshold: `decision_for_profile` reports
    // "code_blocks" as the active method, and `build_chunks` must actually
    // dispatch to the windowed-text fallback for it to be true, not just run
    // `code::code_symbols` unconditionally and mislabel the result.
    let mut body = String::new();
    for i in 0..6000 {
        body.push_str(&format!("fn symbol_{i}() {{ let x = {i}; }}\n"));
    }
    assert!(body.len() > 200_000, "fixture must exceed the threshold");

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Code,
            &body,
            "gen-large-code",
            ChunkingProfile::CodeSymbol,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_profile, "code_symbol");
    assert_eq!(prepared.chunking_method, "code_blocks");
    assert!(!prepared.chunks.is_empty());
    for chunk in &prepared.chunks {
        assert_eq!(chunk.metadata["chunking_fallback"], "size_or_adapter");
        assert_eq!(chunk.metadata["actual_chunking_method"], "code_blocks");
        // The windowed fallback does not stamp code-symbol-specific fields
        // that only `build_prepared_chunk`'s CodeSymbol branch adds from a
        // real `chunk.symbol` -- confirms `code::code_symbols` did not run.
        assert!(!chunk.metadata.contains_key("code_symbol_name"));
    }
}

#[test]
fn small_code_document_from_fragment_prone_adapter_also_uses_windowed_fallback() {
    let mut doc = source_doc(
        ContentKind::Code,
        "fn tiny() { let descriptive_value = 42; println!(\"{descriptive_value}\"); }\n",
    );
    doc.metadata.insert(
        "source_adapter".to_string(),
        serde_json::json!("web_scrape"),
    );

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-fragment"),
            profile: Some(ChunkingProfile::CodeSymbol),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_method, "code_blocks");
    assert_eq!(
        prepared.chunks[0].metadata["chunking_fallback"],
        "size_or_adapter"
    );
}

#[test]
fn large_html_document_still_removes_non_content_payloads() {
    let hydration = "window.__next_f.push(['hydration-payload']);".repeat(20_000);
    let html = format!(
        "<html><body><main>Authorization documentation includes complete useful configuration examples for retrieval.</main><script>{hydration}</script></body></html>"
    );
    let request = request(
        ContentKind::Html,
        &html,
        "gen-large-html",
        ChunkingProfile::HtmlArticle,
    );

    let PrepareSourceDocumentResult::Prepared(prepared) =
        DocumentPreparer::default().prepare(request).unwrap()
    else {
        panic!("expected prepared document")
    };
    let content = prepared
        .chunks
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(content.contains("Authorization documentation"));
    assert!(!content.contains("window.__next_f"));
    assert!(!content.contains("hydration-payload"));
    assert!(prepared.chunks.len() <= 3);
}

#[test]
fn markdown_web_document_projects_structured_payload_into_chunk_metadata() {
    // Dead-code recovery (#298): a `"web"`-family document routed to
    // `MarkdownSections` (the common web/crawl case) never passes
    // `structured_payload` through `build_chunks`'s structured-parse branch --
    // that only runs for `StructuredRecords`/`ApiSchema`. Confirm
    // `project_structured_payload_metadata` still lands it on every chunk (and
    // on the document itself, since `axon-vectors::point::point_payload`
    // builds each point's payload from `document.metadata.clone()`).
    let mut doc = source_doc(
        ContentKind::Markdown,
        "# Intro\nHello from the complete reference documentation for this application.\n\n## More\nText explains additional details of the application configuration.",
    );
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("web"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "jsonld",
        "schema_type": "Article",
        "blob": {"@type": "Article", "headline": "Intro"},
    }));

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-web-structured"),
            profile: Some(ChunkingProfile::MarkdownSections),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_profile, "markdown_sections");
    assert!(prepared.chunks.len() >= 2);
    assert_eq!(prepared.metadata["web_structured_kind"], "Article");
    assert!(
        prepared.metadata["web_structured_blob"]
            .as_str()
            .unwrap()
            .contains("headline")
    );
    for chunk in &prepared.chunks {
        assert_eq!(chunk.metadata["web_structured_kind"], "Article");
        assert!(
            chunk.metadata["web_structured_blob"]
                .as_str()
                .unwrap()
                .contains("headline")
        );
    }
}

#[test]
fn structured_payload_kind_falls_back_when_schema_type_is_absent() {
    // `next_data`/`sveltekit` extractions rarely carry a schema.org
    // `schema_type`; the coarser `kind` field should still surface.
    let mut doc = source_doc(
        ContentKind::Markdown,
        "# Intro\nHello to everyone reading this complete application documentation.",
    );
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("web"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "next_data",
        "blob": {"props": {}},
    }));

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-web-nextdata"),
            profile: Some(ChunkingProfile::MarkdownSections),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.metadata["web_structured_kind"], "next_data");
}

#[test]
fn structured_payload_is_not_projected_outside_the_web_family() {
    // Every non-web adapter leaves `structured_payload` at `None` today, but
    // the projection must still stay family-gated: `web_structured_kind`/
    // `web_structured_blob` are only declared in the `"web"` family's vector
    // payload allowlist, so leaking them onto another family would fail
    // payload validation with `UnknownSourceSpecificField`.
    let mut doc = source_doc(
        ContentKind::Markdown,
        "# Intro\nHello to everyone reading this complete application documentation.",
    );
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("code"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "jsonld",
        "blob": {"@type": "Article"},
    }));

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-nonweb"),
            profile: Some(ChunkingProfile::MarkdownSections),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert!(!prepared.metadata.contains_key("web_structured_kind"));
    assert!(!prepared.metadata.contains_key("web_structured_blob"));
    for chunk in &prepared.chunks {
        assert!(!chunk.metadata.contains_key("web_structured_kind"));
        assert!(!chunk.metadata.contains_key("web_structured_blob"));
    }
}

#[test]
fn oversized_structured_record_is_bounded_before_embedding() {
    // A valid JSON record can contain one very large array. Its structural
    // identity survives while the embedding inputs remain bounded.
    let mut body = String::from("{\"items\":[");
    for i in 0..20_000 {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!("{{\"id\":{i}}}"));
    }
    body.push_str("]}");
    assert!(body.len() > 200_000, "fixture must exceed the threshold");

    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Json,
            &body,
            "gen-large-structured",
            ChunkingProfile::StructuredRecords,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert_eq!(prepared.chunking_profile, "structured_records");
    assert_eq!(prepared.chunking_method, "plain_text_windows");
    assert!(prepared.chunks.len() > 1);
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.content.len() <= 4096)
    );
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.source_range.json_pointer.as_deref() == Some("/items"))
    );
}

#[test]
fn oversized_manifest_line_keeps_exact_source_ranges() {
    let body = format!("name = \"{}\"", "x".repeat(12_000));
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Toml,
            &body,
            "gen-large-manifest",
            ChunkingProfile::CodeManifest,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert!(prepared.chunks.len() > 1);
    for chunk in &prepared.chunks {
        assert!(chunk.content.len() <= 4096);
        let start = chunk.source_range.byte_start.unwrap() as usize;
        let end = chunk.source_range.byte_end.unwrap() as usize;
        assert_eq!(&body[start..end], chunk.content);
    }
}

#[test]
fn oversized_session_turn_keeps_turn_id_and_exact_ranges() {
    let body = format!("{}\nsecond turn", "α".repeat(4_000));
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Transcript,
            &body,
            "gen-large-session",
            ChunkingProfile::SessionTurns,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    let first_turn = prepared
        .chunks
        .iter()
        .filter(|chunk| chunk.source_range.session_turn_id.as_deref() == Some("turn-0"))
        .collect::<Vec<_>>();
    assert!(first_turn.len() > 1);
    for chunk in first_turn {
        let start = chunk.source_range.byte_start.unwrap() as usize;
        let end = chunk.source_range.byte_end.unwrap() as usize;
        assert_eq!(&body[start..end], chunk.content);
        assert!(chunk.content.len() <= 4096);
    }
}

#[test]
fn multi_megabyte_json_record_has_bounded_embedding_chunks() {
    let body = format!("{{\"payload\":\"{}\"}}", "x".repeat(3_000_000));
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Json,
            &body,
            "gen-large-json",
            ChunkingProfile::StructuredRecords,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    assert!(prepared.chunks.len() > 700);
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.content.len() <= 4096)
    );
}

#[test]
fn embedding_backstop_drops_whitespace_only_windows() {
    let body = format!(
        "{}{}{}",
        "useful start context ".repeat(4),
        " ".repeat(6_000),
        "useful end context ".repeat(4)
    );
    let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            &body,
            "gen-spaced-metadata",
            ChunkingProfile::AtomicMetadata,
        ))
        .unwrap()
    else {
        panic!("expected prepared document")
    };
    assert!(prepared.chunks.len() > 1);
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| !chunk.content.trim().is_empty())
    );
}

fn source_doc(content_kind: ContentKind, text: &str) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc-test"),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        canonical_uri: "file:///test.md".to_string(),
        content_kind,
        content: ContentRef::InlineText {
            text: text.to_string(),
        },
        metadata: MetadataMap::new(),
        title: Some("Test doc".to_string()),
        language: None,
        path: Some("test.md".to_string()),
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn request(
    content_kind: ContentKind,
    text: &str,
    generation: &str,
    profile: ChunkingProfile,
) -> PrepareSourceDocumentRequest {
    PrepareSourceDocumentRequest {
        document: source_doc(content_kind, text),
        generation: SourceGenerationId::from(generation),
        profile: Some(profile),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

/// Regression: pre-chunk redaction must run BEFORE the self-parse so parse
/// facts and graph candidates carry line numbers and quotes from the same
/// (redacted) text that range/quote validation later slices. Seen live: a
/// fenced `Authorization: Bearer …` example line was scrubbed after parsing,
/// shifting the text under every later heading candidate and failing
/// preparation with "quote outside source range".
#[test]
fn redacted_content_parses_and_validates_after_scrub() {
    let fake_secret = format!("sk-{}", "a".repeat(28));
    let text = format!(
        "# Title\n\n```bash\ncurl --header \"Authorization: Bearer {fake_secret}\"\n```\n\n## After the secret\n\nBody text.\n"
    );
    let mut request = request(
        ContentKind::Markdown,
        &text,
        "gen-1",
        ChunkingProfile::MarkdownSections,
    );
    // Let the preparer self-parse so heading graph candidates are produced
    // from the content instead of being pre-supplied.
    request.profile = None;

    let result = DocumentPreparer::default()
        .prepare(request)
        .expect("preparation must survive pre-chunk redaction");
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("expected prepared document")
    };

    // The secret is scrubbed from every chunk...
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| !chunk.content.contains(&fake_secret)),
        "pre-chunk redaction must scrub the bearer token"
    );
    // ...and the self-parsed heading candidates (produced from the redacted
    // text) survive range/quote validation, including the heading AFTER the
    // redacted line.
    assert!(
        prepared.graph_candidates.iter().any(|candidate| candidate
            .merge_key
            .as_deref()
            .is_some_and(|key| key.contains("After the secret"))),
        "heading candidates after the redacted line must survive validation; got: {:?}",
        prepared
            .graph_candidates
            .iter()
            .map(|candidate| candidate.merge_key.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn manifest_graph_evidence_survives_real_document_preparation() {
    let samples = [
        (
            "package.json",
            ContentKind::Json,
            "{\n  \"dependencies\": {\n    \"react\": \"19\"\n  },\n  \"scripts\": {\"build\": \"vite build\"}\n}",
            "manifest_dependency",
        ),
        (
            "pom.xml",
            ContentKind::Xml,
            "<project>\n<dependencies><dependency>\n<groupId>org.example</groupId>\n<artifactId>demo</artifactId>\n</dependency></dependencies>\n</project>",
            "manifest_dependency",
        ),
        (
            "deploy.yaml",
            ContentKind::Yaml,
            "apiVersion: apps/v1\nkind:   'Deployment'\nmetadata:\n  name: demo\n",
            "iac_resource",
        ),
    ];
    for (path, kind, text, graph_kind) in samples {
        let mut request = request(kind, text, "gen-manifest", ChunkingProfile::AtomicMetadata);
        request.document.path = Some(path.to_string());
        request.document.canonical_uri = format!("file:///repo/{path}");
        request.profile = None;
        let PrepareSourceDocumentResult::Prepared(prepared) = DocumentPreparer::default()
            .prepare(request)
            .unwrap_or_else(|error| panic!("{path}: {error}"))
        else {
            panic!("{path}: expected prepared document")
        };
        assert!(
            prepared
                .graph_candidates
                .iter()
                .any(|candidate| candidate.kind == graph_kind),
            "{path}: expected {graph_kind} graph evidence"
        );
    }
}

#[test]
fn tutorial_credential_examples_are_preserved_before_chunking() {
    let text = concat!(
        "Authorization: Bearer abc123\n",
        "TOKEN=abc123\n",
        "passwd=hunter2\n",
        "postgres://user:password@localhost/app\n",
    );
    let result = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            text,
            "gen-tutorial",
            ChunkingProfile::PlainTextWindows,
        ))
        .expect("tutorial credential syntax must remain preparable");
    let PrepareSourceDocumentResult::Prepared(prepared) = result else {
        panic!("expected prepared document")
    };
    let joined = prepared
        .chunks
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    for example in [
        "Authorization: Bearer abc123",
        "TOKEN=abc123",
        "passwd=hunter2",
        "postgres://user:password@localhost/app",
    ] {
        assert!(
            joined.contains(example),
            "tutorial example was redacted: {example}"
        );
    }
    assert!(
        prepared
            .warnings
            .iter()
            .all(|warning| warning.code != "document.content.pre_chunk_redacted"),
        "low-confidence tutorial syntax must not trigger pre-chunk redaction"
    );
}

#[test]
fn markdown_windowed_fallback_honors_injected_limits() {
    // Fragment-prone adapter forces the plain_text_windows fallback for the
    // MarkdownSections profile; the fallback must window at the injected
    // markdown limits, not the hardcoded plain-text caps.
    let mut doc = source_doc(
        ContentKind::Markdown,
        &format!("# Scraped\n{}", "fragment text ".repeat(60)),
    );
    doc.metadata.insert(
        "source_adapter".to_string(),
        serde_json::json!("web_scrape"),
    );

    let PrepareSourceDocumentResult::Prepared(prepared) =
        DocumentPreparer::new(DocumentPreparerConfig {
            markdown_max_chars: 96,
            markdown_min_chars: 1,
            markdown_overlap_chars: 0,
            ..DocumentPreparerConfig::default()
        })
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-md-fallback-limits"),
            profile: Some(ChunkingProfile::MarkdownSections),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected prepared document")
    };

    assert!(prepared.chunks.len() > 1);
    assert_eq!(
        prepared.chunks[0].metadata["chunking_fallback"],
        "size_or_adapter"
    );
    assert_eq!(
        prepared.chunks[0].metadata["actual_chunking_method"],
        "plain_text_windows"
    );
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 96),
        "fallback windows must honor the injected markdown max_chars"
    );
}

#[test]
fn acquisition_size_omission_preserves_identity_and_reason() {
    use axon_api::source::{CONTENT_OMISSION_METADATA_KEY, ContentSkipReason};
    let mut input = request(
        ContentKind::PlainText,
        "",
        "gen-omitted",
        ChunkingProfile::MarkdownSections,
    );
    input.document.metadata.insert(
        CONTENT_OMISSION_METADATA_KEY.into(),
        serde_json::json!("size_limit_exceeded"),
    );
    input
        .document
        .metadata
        .insert("binary_policy".into(), serde_json::json!("include"));
    let expected_id = input.document.document_id.clone();
    let PrepareSourceDocumentResult::Skipped(skipped) =
        DocumentPreparer::default().prepare(input).unwrap()
    else {
        panic!("expected size skip")
    };
    assert_eq!(skipped.document_id, expected_id);
    assert_eq!(skipped.reason, ContentSkipReason::SizeLimitExceeded);
}

#[test]
fn content_limit_override_never_raises_configured_ceiling() {
    use axon_api::source::ContentSkipReason;
    for limit in [0, 2, usize::MAX] {
        let preparer = DocumentPreparer::new(DocumentPreparerConfig {
            max_content_bytes: 2,
            ..Default::default()
        })
        .with_content_byte_limit(limit);
        let input = request(
            ContentKind::PlainText,
            "abc",
            "gen-limit",
            ChunkingProfile::MarkdownSections,
        );
        let PrepareSourceDocumentResult::Skipped(skipped) = preparer.prepare(input).unwrap() else {
            panic!("expected size skip")
        };
        assert_eq!(skipped.reason, ContentSkipReason::SizeLimitExceeded);
    }
}

#[test]
fn local_binary_policy_is_consumed_before_prepared_payload_metadata() {
    for policy in ["skip", "metadata", "include"] {
        let mut input = request(
            ContentKind::PlainText,
            "Local text remains searchable with sufficient complete context for embedding.",
            "gen-local-policy",
            ChunkingProfile::MarkdownSections,
        );
        input
            .document
            .metadata
            .insert("binary_policy".into(), serde_json::json!(policy));
        let PrepareSourceDocumentResult::Prepared(prepared) =
            DocumentPreparer::default().prepare(input).unwrap()
        else {
            panic!("expected supported Local text")
        };
        assert!(!prepared.metadata.contains_key("binary_policy"));
        assert!(!prepared.chunks.is_empty());
        assert!(
            prepared
                .chunks
                .iter()
                .all(|chunk| !chunk.metadata.contains_key("binary_policy"))
        );
    }
}

#[test]
fn all_profiles_reject_short_and_punctuation_only_embedding_chunks() {
    let profiles = [
        ChunkingProfile::CodeSymbol,
        ChunkingProfile::CodeManifest,
        ChunkingProfile::MarkdownSections,
        ChunkingProfile::HtmlArticle,
        ChunkingProfile::PlainTextWindows,
        ChunkingProfile::TranscriptSegments,
        ChunkingProfile::StructuredRecords,
        ChunkingProfile::ApiSchema,
        ChunkingProfile::ToolOutput,
        ChunkingProfile::SessionTurns,
        ChunkingProfile::AtomicMetadata,
    ];
    for profile in profiles {
        for source in ["identifier".to_string(), "{}();---\n".repeat(20)] {
            let result = DocumentPreparer::default()
                .prepare(request(
                    ContentKind::PlainText,
                    &source,
                    "gen-quality",
                    profile,
                ))
                .unwrap();
            assert!(
                matches!(result, PrepareSourceDocumentResult::Skipped(_)),
                "{profile}: {source}"
            );
        }
    }
}

#[test]
fn code_without_ast_symbols_preserves_actual_ast_success_or_partial_status() {
    for (source, status) in [
        (
            "fn broken(\n// This malformed declaration retains complete useful source context for retrieval.\n",
            "partial",
        ),
        (
            "// A complete useful source comment without any recognized declarations or symbols.\n",
            "parsed",
        ),
    ] {
        let mut input = request(
            ContentKind::Code,
            source,
            "gen-fallback-method",
            ChunkingProfile::CodeSymbol,
        );
        input.document.path = Some("src/fallback.rs".into());
        input.document.language = Some("rust".into());
        let PrepareSourceDocumentResult::Prepared(prepared) =
            DocumentPreparer::default().prepare(input).unwrap()
        else {
            panic!("expected useful source context")
        };
        assert_eq!(prepared.chunking_method, "tree_sitter");
        assert_eq!(prepared.metadata["code_ast_status"], status);
        assert_eq!(prepared.metadata["code_symbol_count"], 0);
        assert!(
            prepared
                .chunks
                .iter()
                .all(|chunk| chunk.metadata["actual_chunking_method"] == "tree_sitter")
        );
        assert!(
            prepared
                .parse_facts
                .iter()
                .all(|fact| fact.fact_kind != "code_symbol")
        );
    }
}

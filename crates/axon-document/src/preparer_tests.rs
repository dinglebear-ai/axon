use axon_api::source::{
    ChunkId, ContentKind, ContentRef, DocumentId, GraphCandidate, GraphCandidateProducer,
    GraphEvidence, MetadataMap, Severity, SourceDocument, SourceError, SourceGenerationId,
    SourceId, SourceItemKey, SourceParseFacts, SourceRange, SourceWarning,
};
use axon_parse::vertical::{
    VERTICAL_GRAPH_CANDIDATES_METADATA_KEY, VERTICAL_PARSE_FACTS_METADATA_KEY,
};

use crate::{
    ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest,
    preparer::{validate_prepared_document, validate_prepared_document_ranges_against_bounds},
    source_range::bounds_for_text,
    testing::RecordingPreparer,
};

#[test]
fn preparer_builds_prepared_document_from_inline_source_dto() {
    let request = request(
        ContentKind::Markdown,
        "# Intro\nHello\n\n## Next\nWorld",
        "gen-1",
        ChunkingProfile::MarkdownSections,
    );

    let result = DocumentPreparer::default().prepare(request).unwrap();
    let prepared = result.document;

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
fn recording_preparer_records_requests_and_returns_real_prepared_documents() {
    let mut recorder = RecordingPreparer::new(DocumentPreparer::default());
    let request = request(
        ContentKind::PlainText,
        "alpha\r\n\r\nbeta",
        "gen-fake",
        ChunkingProfile::PlainTextWindows,
    );

    let result = recorder.prepare(request.clone()).unwrap();

    assert_eq!(recorder.requests(), &[request]);
    assert_eq!(result.document.chunking_profile, "plain_text_windows");
    assert_eq!(result.document.chunks.len(), 2);
}

#[test]
fn preparer_rejects_empty_prepared_documents() {
    let request = request(
        ContentKind::PlainText,
        " \n\n\t",
        "gen-empty",
        ChunkingProfile::PlainTextWindows,
    );

    let error = DocumentPreparer::default().prepare(request).unwrap_err();

    assert!(error.contains("prepared document has no chunks"));
}

#[test]
fn validate_prepared_document_rejects_duplicate_chunk_identity() {
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "alpha\n\nbeta",
            "gen-duplicates",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
        .document;
    let mut invalid = prepared;
    invalid.chunks[1].chunk_id = invalid.chunks[0].chunk_id.clone();
    invalid.chunks[1].chunk_key = invalid.chunks[0].chunk_key.clone();

    let error = validate_prepared_document(&invalid).unwrap_err();

    assert!(error.contains("duplicate chunk id"));
    assert!(error.contains("duplicate chunk key"));
}

#[test]
fn validate_prepared_document_rejects_impossible_ranges_and_empty_content() {
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "alpha",
            "gen-invalid-range",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
        .document;
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
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "PORT=3000\n",
            "gen-bounds",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
        .document;
    let mut invalid = prepared;
    invalid.chunks[0].source_range.line_start = Some(9000);
    invalid.chunks[0].source_range.line_end = Some(9001);

    let bounds = bounds_for_text("PORT=3000\n");
    let err =
        validate_prepared_document_ranges_against_bounds(&invalid, &bounds, Some("PORT=3000\n"))
            .expect_err("range outside normalized document rejected");
    assert!(err.contains("outside normalized document"));
}

#[test]
fn preparer_rejects_graph_evidence_ranges_outside_normalized_document() {
    let source_text = "FROM alpine:3\n";
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            source_text,
            "gen-graph-bounds",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
        .document;
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
                line_start: Some(2),
                line_end: Some(2),
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
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::PlainText,
            "first\nsecond\n",
            "gen-time-turn",
            ChunkingProfile::PlainTextWindows,
        ))
        .unwrap()
        .document;
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

    let prepared = DocumentPreparer::default()
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
        .document;

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
pub fn alpha() {}\n\
\n\
================================================================\n\
File: src/main.rs\n\
================================================================\n\
fn main() {}\n";
    let result = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Code,
            packed,
            "gen-repomix",
            ChunkingProfile::CodeSymbol,
        ))
        .unwrap();

    let chunks = result.document.chunks;

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

    let prepared = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: source_doc(ContentKind::PlainText, "body"),
            generation: SourceGenerationId::from("gen-artifacts"),
            profile: Some(ChunkingProfile::PlainTextWindows),
            parse_facts: vec![fact.clone()],
            graph_candidates: vec![candidate.clone()],
            warnings: vec![warning.clone()],
            errors: vec![error.clone()],
        })
        .unwrap()
        .document;

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
    let mut doc = source_doc(ContentKind::Markdown, "# Axon\n\nRepository metadata.");
    doc.metadata.insert(
        VERTICAL_PARSE_FACTS_METADATA_KEY.to_string(),
        serde_json::to_value(vec![fact.clone()]).unwrap(),
    );
    doc.metadata.insert(
        VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(),
        serde_json::to_value(vec![candidate.clone()]).unwrap(),
    );

    let prepared = DocumentPreparer::default()
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
        .document;

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
    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Json,
            "{\"broken\":",
            "gen-structured",
            ChunkingProfile::StructuredRecords,
        ))
        .unwrap()
        .document;

    assert_eq!(prepared.chunks.len(), 1);
    assert_eq!(
        prepared.chunks[0].metadata["chunking_fallback"],
        "atomic_text"
    );
    assert_eq!(prepared.warnings.len(), 1);
    assert_eq!(prepared.warnings[0].code, "chunk.structured_parse_failed");
}

#[test]
fn non_inline_content_degrades_to_atomic_metadata_chunk() {
    let mut doc = source_doc(ContentKind::BinaryMetadata, "");
    doc.content = ContentRef::External {
        uri: "artifact://source/raw".to_string(),
        integrity: Some("sha256:abc".to_string()),
    };
    let prepared = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: doc,
            generation: SourceGenerationId::from("gen-external"),
            profile: Some(ChunkingProfile::PlainTextWindows),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
        .document;

    assert_eq!(prepared.chunking_profile, "atomic_metadata");
    assert_eq!(prepared.chunks.len(), 1);
    assert!(
        prepared.chunks[0]
            .content
            .contains("external content reference")
    );
    assert_eq!(
        prepared.warnings[0].code,
        "document.content.external_fallback"
    );
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

    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Code,
            &body,
            "gen-large-code",
            ChunkingProfile::CodeSymbol,
        ))
        .unwrap()
        .document;

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
    let mut doc = source_doc(ContentKind::Code, "fn tiny() {}\n");
    doc.metadata.insert(
        "source_adapter".to_string(),
        serde_json::json!("web_scrape"),
    );

    let prepared = DocumentPreparer::default()
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
        .document;

    assert_eq!(prepared.chunking_method, "code_blocks");
    assert_eq!(
        prepared.chunks[0].metadata["chunking_fallback"],
        "size_or_adapter"
    );
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
        "# Intro\nHello from docs.\n\n## More\nText.",
    );
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("web"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "jsonld",
        "schema_type": "Article",
        "blob": {"@type": "Article", "headline": "Intro"},
    }));

    let prepared = DocumentPreparer::default()
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
        .document;

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
    let mut doc = source_doc(ContentKind::Markdown, "# Intro\nHello.");
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("web"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "next_data",
        "blob": {"props": {}},
    }));

    let prepared = DocumentPreparer::default()
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
        .document;

    assert_eq!(prepared.metadata["web_structured_kind"], "next_data");
}

#[test]
fn structured_payload_is_not_projected_outside_the_web_family() {
    // Every non-web adapter leaves `structured_payload` at `None` today, but
    // the projection must still stay family-gated: `web_structured_kind`/
    // `web_structured_blob` are only declared in the `"web"` family's vector
    // payload allowlist, so leaking them onto another family would fail
    // payload validation with `UnknownSourceSpecificField`.
    let mut doc = source_doc(ContentKind::Markdown, "# Intro\nHello.");
    doc.metadata
        .insert("source_family".to_string(), serde_json::json!("code"));
    doc.structured_payload = Some(serde_json::json!({
        "kind": "jsonld",
        "blob": {"@type": "Article"},
    }));

    let prepared = DocumentPreparer::default()
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
        .document;

    assert!(!prepared.metadata.contains_key("web_structured_kind"));
    assert!(!prepared.metadata.contains_key("web_structured_blob"));
    for chunk in &prepared.chunks {
        assert!(!chunk.metadata.contains_key("web_structured_kind"));
        assert!(!chunk.metadata.contains_key("web_structured_blob"));
    }
}

#[test]
fn unwired_profile_ignores_size_and_keeps_reporting_its_primary_method() {
    // StructuredRecords has no wired size fallback: even past the threshold,
    // both the reported method and the actual chunker stay on the profile's
    // primary structured parser (this fixture is valid JSON, so it does not
    // hit the separate parse-failure fallback path either).
    let mut body = String::from("{\"items\":[");
    for i in 0..20_000 {
        if i > 0 {
            body.push(',');
        }
        body.push_str(&format!("{{\"id\":{i}}}"));
    }
    body.push_str("]}");
    assert!(body.len() > 200_000, "fixture must exceed the threshold");

    let prepared = DocumentPreparer::default()
        .prepare(request(
            ContentKind::Json,
            &body,
            "gen-large-structured",
            ChunkingProfile::StructuredRecords,
        ))
        .unwrap()
        .document;

    assert_eq!(prepared.chunking_profile, "structured_records");
    assert_eq!(prepared.chunking_method, "structured_records");
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
    let text = "# Title\n\n```bash\ncurl --header \"Authorization: Bearer secret-token-value\"\n```\n\n## After the secret\n\nBody text.\n";
    let mut request = request(
        ContentKind::Markdown,
        text,
        "gen-1",
        ChunkingProfile::MarkdownSections,
    );
    // Let the preparer self-parse so heading graph candidates are produced
    // from the content instead of being pre-supplied.
    request.profile = None;

    let result = DocumentPreparer::default()
        .prepare(request)
        .expect("preparation must survive pre-chunk redaction");
    let prepared = result.document;

    // The secret is scrubbed from every chunk...
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| !chunk.content.contains("secret-token-value")),
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

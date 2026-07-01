use axon_api::source::{
    ChunkId, ContentKind, ContentRef, DocumentId, GraphCandidate, GraphCandidateProducer,
    MetadataMap, Severity, SourceDocument, SourceError, SourceGenerationId, SourceId,
    SourceItemKey, SourceParseFacts, SourceWarning,
};

use crate::{
    ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest,
    preparer::validate_prepared_document, testing::RecordingPreparer,
};

#[test]
fn preparer_builds_prepared_document_from_inline_source_dto() {
    let request = PrepareSourceDocumentRequest {
        document: source_doc(ContentKind::Markdown, "# Intro\nHello\n\n## Next\nWorld"),
        generation: SourceGenerationId::from("gen-1"),
        profile: Some(ChunkingProfile::MarkdownSections),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

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
    let request = PrepareSourceDocumentRequest {
        document: source_doc(ContentKind::PlainText, "alpha\r\n\r\nbeta"),
        generation: SourceGenerationId::from("gen-fake"),
        profile: Some(ChunkingProfile::PlainTextWindows),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let result = recorder.prepare(request.clone()).unwrap();

    assert_eq!(recorder.requests(), &[request]);
    assert_eq!(result.document.chunking_profile, "plain_text_windows");
    assert_eq!(result.document.chunks.len(), 2);
}

#[test]
fn preparer_rejects_empty_prepared_documents() {
    let request = PrepareSourceDocumentRequest {
        document: source_doc(ContentKind::PlainText, " \n\n\t"),
        generation: SourceGenerationId::from("gen-empty"),
        profile: Some(ChunkingProfile::PlainTextWindows),
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let error = DocumentPreparer::default().prepare(request).unwrap_err();

    assert!(error.contains("prepared document has no chunks"));
}

#[test]
fn validate_prepared_document_rejects_duplicate_chunk_identity() {
    let prepared = DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document: source_doc(ContentKind::PlainText, "alpha\n\nbeta"),
            generation: SourceGenerationId::from("gen-duplicates"),
            profile: Some(ChunkingProfile::PlainTextWindows),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
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
        .prepare(PrepareSourceDocumentRequest {
            document: source_doc(ContentKind::PlainText, "alpha"),
            generation: SourceGenerationId::from("gen-invalid-range"),
            profile: Some(ChunkingProfile::PlainTextWindows),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
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
        .prepare(PrepareSourceDocumentRequest {
            document: source_doc(ContentKind::Code, packed),
            generation: SourceGenerationId::from("gen-repomix"),
            profile: Some(ChunkingProfile::CodeSymbol),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
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

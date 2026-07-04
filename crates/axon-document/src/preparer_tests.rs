use axon_api::source::{
    ChunkId, ContentKind, ContentRef, DocumentId, GraphCandidate, GraphCandidateProducer,
    MetadataMap, Severity, SourceDocument, SourceError, SourceGenerationId, SourceId,
    SourceItemKey, SourceParseFacts, SourceWarning,
};

use crate::{
    ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest,
    preparer::{validate_prepared_document, validate_prepared_document_ranges},
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

    let err = validate_prepared_document_ranges(&invalid)
        .expect_err("range outside normalized document rejected");
    assert!(err.contains("outside normalized document"));
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

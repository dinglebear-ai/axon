use axon_api::source::{
    ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument, SourceGenerationId, SourceId,
    SourceItemKey,
};

use crate::{
    ChunkingProfile, DocumentPreparer, PrepareSourceDocumentRequest, testing::FakePreparer,
};

#[test]
fn preparer_builds_prepared_document_from_inline_source_dto() {
    let request = PrepareSourceDocumentRequest {
        document: source_doc(ContentKind::Markdown, "# Intro\nHello\n\n## Next\nWorld"),
        generation: SourceGenerationId::from("gen-1"),
        profile: Some(ChunkingProfile::MarkdownSections),
    };

    let result = DocumentPreparer::default().prepare(request).unwrap();
    let prepared = result.document;

    assert_eq!(prepared.document_id, DocumentId::from("doc-test"));
    assert_eq!(prepared.source_id, SourceId::from("src-test"));
    assert_eq!(prepared.source_item_key, SourceItemKey::from("item-test"));
    assert_eq!(prepared.generation, SourceGenerationId::from("gen-1"));
    assert_eq!(prepared.chunking_profile, "markdown_sections");
    assert_eq!(prepared.chunks.len(), 2);
    assert_eq!(prepared.chunks[0].chunk_key, "doc-test:0000");
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
fn fake_preparer_records_requests_and_returns_real_prepared_documents() {
    let mut fake = FakePreparer::new(DocumentPreparer::default());
    let request = PrepareSourceDocumentRequest {
        document: source_doc(ContentKind::PlainText, "alpha\r\n\r\nbeta"),
        generation: SourceGenerationId::from("gen-fake"),
        profile: Some(ChunkingProfile::PlainTextWindows),
    };

    let result = fake.prepare(request.clone()).unwrap();

    assert_eq!(fake.requests(), &[request]);
    assert_eq!(result.document.chunking_profile, "plain_text_windows");
    assert_eq!(result.document.chunks.len(), 2);
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

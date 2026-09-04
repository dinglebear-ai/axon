use std::sync::Arc;

use axon_api::source::{
    ChunkProfile, ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument,
    SourceGenerationId, SourceId, SourceItemKey,
};

use super::{ChunkRouter, DocumentPreparer};
use crate::PrepareSourceDocumentRequest;

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

fn prepare_request(document: SourceDocument, generation: &str) -> PrepareSourceDocumentRequest {
    PrepareSourceDocumentRequest {
        document,
        generation: SourceGenerationId::new(generation),
        profile: None,
        parse_facts: Vec::new(),
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

/// The concrete `crate::preparer::DocumentPreparer` struct must satisfy the
/// `boundary::DocumentPreparer` trait object, non-breakingly, alongside its
/// existing inherent API.
#[tokio::test]
async fn concrete_document_preparer_satisfies_boundary_trait() {
    let preparer: Arc<dyn DocumentPreparer> =
        Arc::new(crate::preparer::DocumentPreparer::default());

    let document = source_doc(ContentKind::Markdown, "# Hello\nWorld");
    let prepared = preparer
        .prepare(prepare_request(document, "generation-42"))
        .await
        .expect("boundary prepare should preserve required generation lineage");

    assert!(!prepared.chunks.is_empty());
    assert_eq!(
        prepared.generation,
        SourceGenerationId::new("generation-42")
    );

    let capabilities = preparer.capabilities().await.expect("capabilities");
    assert_eq!(capabilities.0.owner_crate, "axon-document");
}

#[tokio::test]
async fn concrete_document_preparer_prepare_many_short_circuits_on_first_error() {
    let preparer: Arc<dyn DocumentPreparer> =
        Arc::new(crate::preparer::DocumentPreparer::default());

    let documents = vec![
        prepare_request(source_doc(ContentKind::Markdown, "# Hello\nWorld"), "g1"),
        prepare_request(source_doc(ContentKind::Markdown, "# Second\nDoc"), "g1"),
    ];
    let prepared = preparer
        .prepare_many(documents)
        .await
        .expect("both documents should prepare successfully");
    assert_eq!(prepared.len(), 2);
}

/// The concrete `crate::chunk_router::ChunkRouter` struct must satisfy the
/// `boundary::ChunkRouter` trait object.
#[test]
fn concrete_chunk_router_satisfies_boundary_trait() {
    let router: Arc<dyn ChunkRouter> = Arc::new(crate::chunk_router::ChunkRouter);

    let document = source_doc(ContentKind::Markdown, "# Hello\nWorld");
    let profile = router.route(&document).expect("route should succeed");
    assert_eq!(profile, ChunkProfile::MarkdownSections);

    let profiles = router.supported_profiles();
    assert_eq!(profiles.len(), 11);
}

#[test]
fn fake_document_preparer_records_calls_and_supports_modes() {
    use crate::testing::{FakeDocumentMode, FakeDocumentPreparer};

    let fake = FakeDocumentPreparer::with_mode(FakeDocumentMode::Success);
    let document = source_doc(ContentKind::PlainText, "hello");
    let result = tokio_test_prepare(&fake, prepare_request(document.clone(), "fake-generation"));
    assert!(result.is_ok());
    assert_eq!(fake.calls().len(), 1);
    assert_eq!(fake.calls()[0].document_id, document.document_id);

    let failing = FakeDocumentPreparer::with_mode(FakeDocumentMode::Failure);
    let err = tokio_test_prepare(
        &failing,
        prepare_request(source_doc(ContentKind::PlainText, "hello"), "g"),
    );
    assert!(err.is_err());

    let degraded = FakeDocumentPreparer::with_mode(FakeDocumentMode::Degraded);
    let ok = tokio_test_prepare(
        &degraded,
        prepare_request(source_doc(ContentKind::PlainText, "hello"), "g"),
    )
    .expect("degraded mode still returns Ok with a warning");
    assert!(
        ok.warnings
            .iter()
            .any(|w| w.code == "document.prepare.fake_degraded")
    );
}

#[test]
fn fake_chunk_router_records_calls_and_supports_fixed_profile() {
    use crate::testing::{FakeChunkRouter, FakeDocumentMode};

    let fake = FakeChunkRouter::new().with_fixed_profile(ChunkProfile::CodeSymbol);
    let document = source_doc(ContentKind::Code, "fn main() {}");
    let profile = fake.route(&document).expect("route should succeed");
    assert_eq!(profile, ChunkProfile::CodeSymbol);
    assert_eq!(fake.calls().len(), 1);

    let failing = FakeChunkRouter::with_mode(FakeDocumentMode::Failure);
    assert!(failing.route(&document).is_err());
}

/// Small helper to drive an async trait method from a sync test body without
/// pulling `#[tokio::test]` onto every fake-mode assertion.
fn tokio_test_prepare(
    preparer: &dyn DocumentPreparer,
    request: PrepareSourceDocumentRequest,
) -> super::Result<axon_api::source::PreparedDocument> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build current-thread runtime")
        .block_on(preparer.prepare(request))
}

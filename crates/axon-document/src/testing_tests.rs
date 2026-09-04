use axon_api::source::{
    ChunkProfile, ContentKind, ContentRef, DocumentId, MetadataMap, SourceDocument, SourceId,
    SourceItemKey,
};

use super::*;
use crate::boundary::{ChunkRouter as _, DocumentPreparer as _};

fn source_doc() -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc-test"),
        source_id: SourceId::from("src-test"),
        source_item_key: SourceItemKey::from("item-test"),
        canonical_uri: "file:///test.md".to_string(),
        content_kind: ContentKind::PlainText,
        content: ContentRef::InlineText {
            text: "hello".to_string(),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

#[tokio::test]
async fn fake_document_preparer_default_is_success() {
    let fake = FakeDocumentPreparer::new();
    let prepared = fake.prepare(source_doc()).await.expect("default success");
    assert!(prepared.warnings.is_empty());
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn fake_document_preparer_prepare_many_records_all_calls() {
    let fake = FakeDocumentPreparer::new();
    let docs = vec![source_doc(), source_doc()];
    let prepared = fake.prepare_many(docs).await.expect("prepare_many ok");
    assert_eq!(prepared.len(), 2);
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn fake_chunk_router_default_profile() {
    let fake = FakeChunkRouter::new();
    let profile = fake.route(&source_doc()).expect("route ok");
    assert_eq!(profile, ChunkProfile::PlainTextWindows);
}

#[test]
fn fake_chunk_router_supported_profiles_nonempty() {
    let fake = FakeChunkRouter::new();
    assert!(!fake.supported_profiles().is_empty());
}

use std::sync::Arc;

use super::*;

/// Compile-level assertion that the production impl satisfies the trait
/// object (no live providers exercised).
#[allow(dead_code)]
fn assert_document_service_impl_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn DocumentService> {
    Arc::new(DocumentServiceImpl::new(ctx))
}

fn empty_range() -> axon_api::source::SourceRange {
    axon_api::source::SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
        time_end_ms: None,
    }
}

fn sample_document() -> DocumentDetail {
    let document_id = DocumentId::new("doc-1");
    let chunk = ChunkSummary {
        chunk_id: axon_api::source::ChunkId::new("chunk-1"),
        document_id: document_id.clone(),
        chunk_index: 0,
        chunk_locator: axon_api::source::ChunkLocator {
            canonical_uri: "https://example.com".to_string(),
            path: None,
            heading_path: Vec::new(),
            symbol: None,
            range: empty_range(),
        },
        source_range: empty_range(),
        metadata: axon_api::source::MetadataMap::new(),
        graph_refs: Vec::new(),
        vector_refs: Vec::new(),
    };
    DocumentDetail {
        document_id: document_id.clone(),
        source_id: axon_api::source::SourceId::new("source-1"),
        source_item_key: axon_api::source::SourceItemKey::new("item-1"),
        status: axon_api::source::DocumentLifecycleStatus::Published,
        chunk_count: 1,
        vector_point_count: 1,
        content_kind: None,
        title: Some("Example".to_string()),
        path: None,
        graph_refs: Vec::new(),
        generation: axon_api::source::SourceGenerationId::new("gen-1"),
        metadata: axon_api::source::MetadataMap::new(),
        chunk_summary: chunk.clone(),
        vector_keys: Vec::new(),
        chunks: vec![chunk],
        source: None,
        graph: Vec::new(),
    }
}

#[tokio::test]
async fn fake_document_service_list_through_trait_object() {
    let fake: Arc<dyn DocumentService> = Arc::new(FakeDocumentService::new());
    let seedable = FakeDocumentService::new();
    seedable.seed(sample_document());
    let list_fake: Arc<dyn DocumentService> = Arc::new(seedable);

    let empty = fake
        .list(DocumentListRequest {
            source_id: None,
            status: None,
            generation: None,
            content_kind: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list should succeed");
    assert!(empty.items.is_empty());
    assert_eq!(empty.total, Some(0));

    let page = list_fake
        .list(DocumentListRequest {
            source_id: None,
            status: None,
            generation: None,
            content_kind: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list should succeed");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].document_id.0, "doc-1");
    assert_eq!(page.total, Some(1));
}

#[tokio::test]
async fn fake_document_service_get_after_seed() {
    let fake = FakeDocumentService::new();
    fake.seed(sample_document());
    let doc = fake
        .get(DocumentId::new("doc-1"))
        .await
        .expect("document should exist");
    assert_eq!(doc.title.as_deref(), Some("Example"));
}

#[tokio::test]
async fn fake_document_service_chunks_and_chunk() {
    let fake = FakeDocumentService::new();
    fake.seed(sample_document());

    let chunks = fake
        .chunks(ChunkListRequest {
            document_id: DocumentId::new("doc-1"),
            include_content: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("chunks should succeed");
    assert_eq!(chunks.items.len(), 1);

    let chunk = fake
        .chunk(ChunkGetRequest {
            document_id: DocumentId::new("doc-1"),
            chunk_id: axon_api::source::ChunkId::new("chunk-1"),
            include_content: None,
        })
        .await
        .expect("chunk should succeed");
    assert_eq!(chunk.chunk_id.0, "chunk-1");
}

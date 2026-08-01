use std::sync::Arc;

use axon_api::source::*;
use axon_core::boundary::{DocumentCache, FakeCoreBoundaries};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;

use super::reuse_cached_document;
use crate::context::TargetLocalSourceRuntime;

#[tokio::test]
async fn reused_document_is_retargeted_into_the_next_cache_generation() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        Arc::new(FakeVectorStore::new("fake-vector")),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    runtime.document_cache = core.clone();
    let source_id = SourceId::new("src_reuse");
    let item_key = SourceItemKey::new("https://example.test/page");
    let previous = SourceGenerationId::new("gen_previous");
    let next = SourceGenerationId::new("gen_next");
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "source_generation".to_string(),
        serde_json::json!(previous.0),
    );
    metadata.insert(
        "committed_generation".to_string(),
        serde_json::json!(previous.0),
    );
    core.put(
        DocumentCacheKey {
            source_id: source_id.clone(),
            source_item_key: item_key.clone(),
            generation: Some(previous.clone()),
        },
        CachedDocument {
            document: SourceDocument {
                document_id: DocumentId::new("doc_reuse"),
                source_id: source_id.clone(),
                source_item_key: item_key.clone(),
                canonical_uri: item_key.0.clone(),
                content_kind: ContentKind::Markdown,
                content: ContentRef::InlineText {
                    text: "cached body".to_string(),
                },
                title: None,
                language: None,
                path: None,
                mime_type: Some("text/markdown".to_string()),
                structured_payload: None,
                metadata,
                artifact_id: None,
                chunk_hints: Vec::new(),
                parser_hints: Vec::new(),
            },
            cached_at: Timestamp("2026-07-30T00:00:00Z".to_string()),
        },
    )
    .await
    .expect("seed previous cache");
    let diff = SourceManifestDiff {
        header: StageResultHeader {
            job_id: JobId::new(uuid::Uuid::nil()),
            stage_id: StageId::new(uuid::Uuid::nil()),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: Timestamp("2026-07-31T00:00:00Z".to_string()),
            completed_at: Some(Timestamp("2026-07-31T00:00:00Z".to_string())),
            counts: StageCounts {
                items_total: None,
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: source_id.clone(),
        previous_generation: Some(previous),
        next_generation: next.clone(),
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    };

    assert!(
        reuse_cached_document(&runtime, &diff, &item_key)
            .await
            .unwrap()
    );
    let copied = core
        .get(DocumentCacheKey {
            source_id,
            source_item_key: item_key,
            generation: Some(next),
        })
        .await
        .unwrap()
        .expect("next-generation cache entry");
    assert_eq!(
        copied.document.content,
        ContentRef::InlineText {
            text: "cached body".to_string()
        }
    );
    assert!(!copied.document.metadata.contains_key("source_generation"));
    assert!(
        !copied
            .document
            .metadata
            .contains_key("committed_generation")
    );
}

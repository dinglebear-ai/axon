use axon_api::source::*;
use uuid::Uuid;

use crate::store::{FakeLedgerStore, LedgerStore};

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src_a"),
        canonical_uri: "file:///repo".to_string(),
        display_name: "repo".to_string(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 1,
            vector_points_total: 1,
            bytes_total: 12,
        },
        created_at: ts(),
        updated_at: ts(),
        tags: Vec::new(),
        watch_id: None,
        last_job_id: None,
    }
}

fn manifest(hash: &str) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_a"),
        generation: SourceGenerationId::new(format!("gen_{hash}")),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items: vec![ManifestItem {
            source_id: SourceId::new("src_a"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            canonical_uri: "file:///repo/src/lib.rs".to_string(),
            item_kind: ItemKind::LocalFile,
            content_kind: Some(ContentKind::Code),
            display_path: Some("src/lib.rs".to_string()),
            parent_key: None,
            size_bytes: Some(12),
            content_hash: Some(hash.to_string()),
            mtime: Some(ts()),
            version: None,
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        }],
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn fake_ledger_diffs_manifests_and_tracks_committed_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let first = ledger.diff_manifest(manifest("a")).await.unwrap();
    assert_eq!(first.counts.added, 1);
    ledger.put_manifest(manifest("a")).await.unwrap();

    let generation = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    ledger.publish_generation(generation.clone()).await.unwrap();

    let refreshed = ledger.diff_manifest(manifest("b")).await.unwrap();
    assert_eq!(refreshed.counts.modified, 1);
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(generation.generation)
    );
}

#[tokio::test]
async fn fake_ledger_owns_document_status_and_cleanup_debt() {
    let ledger = FakeLedgerStore::new();
    let status = DocumentStatus {
        document_id: DocumentId::new("doc-a"),
        source_id: SourceId::new("src_a"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_1"),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts(),
        chunk_count: 1,
        vector_point_count: 1,
        error: None,
        cleanup_status: None,
    };

    ledger.update_document_status(status.clone()).await.unwrap();
    assert_eq!(
        ledger.document_status(&DocumentId::new("doc-a")).await,
        Some(status)
    );

    ledger
        .record_cleanup_debt(CleanupDebt {
            debt_id: CleanupDebtId::new("debt-a"),
            job_id: JobId::new(Uuid::from_u128(1)),
            source_id: SourceId::new("src_a"),
            generation: Some(SourceGenerationId::new("gen_1")),
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::Document {
                document_id: DocumentId::new("doc-a"),
            },
            status: LifecycleStatus::Pending,
            created_at: ts(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        })
        .await
        .unwrap();
    assert_eq!(ledger.cleanup_debt_count().await, 1);
    ledger.reset().await.unwrap();
    assert_eq!(ledger.cleanup_debt_count().await, 0);
}

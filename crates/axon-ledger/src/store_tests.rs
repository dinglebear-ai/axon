use axon_api::source::*;
use uuid::Uuid;

use crate::store::{FakeLedgerStore, LedgerStore};

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn ts_at(second: u32) -> Timestamp {
    Timestamp(format!("2026-07-01T00:00:{second:02}Z"))
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
    manifest_with_freshness(hash, None, ts())
}

fn manifest_with_freshness(hash: &str, version: Option<&str>, mtime: Timestamp) -> SourceManifest {
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
            mtime: Some(mtime),
            version: version.map(str::to_string),
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        }],
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_for_generation(generation: &SourceGeneration, hash: &str) -> SourceManifest {
    let mut manifest = manifest(hash);
    manifest.generation = generation.generation.clone();
    manifest
}

fn manifest_with_items(generation: &str, items: Vec<ManifestItem>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_a"),
        generation: SourceGenerationId::new(generation),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items,
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_item(path: &str, hash: &str) -> ManifestItem {
    ManifestItem {
        source_id: SourceId::new("src_a"),
        source_item_key: SourceItemKey::new(path),
        canonical_uri: format!("file:///repo/{path}"),
        item_kind: ItemKind::LocalFile,
        content_kind: Some(ContentKind::Code),
        display_path: Some(path.to_string()),
        parent_key: None,
        size_bytes: Some(12),
        content_hash: Some(hash.to_string()),
        mtime: Some(ts()),
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }
}

fn completed_generation(mut generation: SourceGeneration) -> SourceGeneration {
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Committed;
    generation
}

fn completed_generation_for_manifest(manifest: &SourceManifest) -> SourceGeneration {
    SourceGeneration {
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        status: LifecycleStatus::Completed,
        publish_state: PublishState::Committed,
        created_at: ts(),
        published_at: None,
        item_counts: ItemCounts {
            added: manifest.items.len() as u64,
            modified: 0,
            removed: 0,
            unchanged: 0,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation: None,
    }
}

fn lease_request(key: &str, owner: &str) -> LeaseRequest {
    lease_request_ttl(key, owner, 30)
}

fn lease_request_ttl(key: &str, owner: &str, ttl_seconds: u64) -> LeaseRequest {
    LeaseRequest {
        lease_key: key.to_string(),
        owner_id: owner.to_string(),
        ttl_seconds,
        job_id: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn fake_ledger_diffs_manifests_and_tracks_committed_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let first = ledger.diff_manifest(manifest("a")).await.unwrap();
    assert_eq!(first.counts.added, 1);
    let first_manifest = manifest("a");
    ledger.put_manifest(first_manifest.clone()).await.unwrap();

    let generation = completed_generation_for_manifest(&first_manifest);
    ledger.publish_generation(generation.clone()).await.unwrap();

    let refreshed = ledger.diff_manifest(manifest("b")).await.unwrap();
    assert_eq!(refreshed.counts.modified, 1);
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(generation.generation)
    );
}

#[tokio::test]
async fn fake_ledger_diffs_only_against_committed_generation() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    ledger.put_manifest(manifest("uncommitted")).await.unwrap();

    let diff = ledger.diff_manifest(manifest("next")).await.unwrap();
    assert_eq!(diff.previous_generation, None);
    assert_eq!(diff.counts.added, 1);
    assert_eq!(diff.counts.modified, 0);
    assert_eq!(diff.counts.unchanged, 0);
}

#[tokio::test]
async fn fake_ledger_scopes_generation_ids_per_source() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let mut src_b = source();
    src_b.source_id = SourceId::new("src_b");
    ledger.upsert_source(src_b).await.unwrap();

    let src_a_first = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    let src_b_first = ledger
        .create_generation(SourceId::new("src_b"))
        .await
        .unwrap();
    assert_eq!(src_a_first.generation, SourceGenerationId::new("gen_1"));
    assert_eq!(src_b_first.generation, SourceGenerationId::new("gen_1"));

    ledger
        .put_manifest(manifest_for_generation(&src_a_first, "src-a-first"))
        .await
        .unwrap();
    ledger
        .publish_generation(completed_generation(src_a_first.clone()))
        .await
        .unwrap();
    let src_a_second = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    assert_eq!(src_a_second.generation, SourceGenerationId::new("gen_2"));
    assert_eq!(
        src_a_second.previous_generation,
        Some(src_a_first.generation)
    );
}

#[tokio::test]
async fn fake_ledger_diffs_version_and_mtime_changes() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let previous = manifest_with_freshness("a", Some("v1"), ts());
    ledger.put_manifest(previous.clone()).await.unwrap();
    ledger
        .publish_generation(completed_generation_for_manifest(&previous))
        .await
        .unwrap();

    let version_changed = ledger
        .diff_manifest(manifest_with_freshness("a", Some("v2"), ts()))
        .await
        .unwrap();
    assert_eq!(version_changed.counts.modified, 1);
    assert_eq!(version_changed.counts.unchanged, 0);

    let mtime_changed = ledger
        .diff_manifest(manifest_with_freshness(
            "a",
            Some("v1"),
            Timestamp("2026-07-02T00:00:00Z".to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(mtime_changed.counts.modified, 1);
    assert_eq!(mtime_changed.counts.unchanged, 0);
}

#[tokio::test]
async fn fake_ledger_rejects_non_publishable_generation_statuses() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let running = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();

    let error = ledger
        .publish_generation(running.clone())
        .await
        .unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_not_publishable"
    );
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        None
    );

    ledger
        .put_manifest(manifest_for_generation(&running, "running"))
        .await
        .unwrap();
    ledger
        .publish_generation(completed_generation(running.clone()))
        .await
        .unwrap();
    assert_eq!(
        ledger.committed_generation(&SourceId::new("src_a")).await,
        Some(running.generation)
    );
}

#[tokio::test]
async fn fake_ledger_owns_document_status_and_cleanup_debt() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
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

#[tokio::test]
async fn fake_cleanup_debt_uses_natural_key_and_terminal_state_is_monotonic() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let mut debt = CleanupDebt {
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
    };

    ledger.record_cleanup_debt(debt.clone()).await.unwrap();

    debt.debt_id = CleanupDebtId::new("debt-b");
    ledger.record_cleanup_debt(debt.clone()).await.unwrap();
    assert_eq!(ledger.cleanup_debt_count().await, 1);

    debt.status = LifecycleStatus::Completed;
    debt.completed_at = Some(ts_at(10));
    ledger.record_cleanup_debt(debt.clone()).await.unwrap();

    debt.status = LifecycleStatus::Pending;
    debt.completed_at = None;
    ledger.record_cleanup_debt(debt).await.unwrap();

    let stored = ledger
        .cleanup_debt(&CleanupDebtId::new("debt-b"))
        .await
        .expect("stored debt");
    assert_eq!(stored.status, LifecycleStatus::Completed);
    assert_eq!(stored.completed_at, Some(ts_at(10)));
}

#[tokio::test]
async fn fake_publish_creates_cleanup_debt_for_removed_items() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let gen1 = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    ledger
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![
                manifest_item("README.md", "same"),
                manifest_item("src/old.rs", "removed"),
            ],
        ))
        .await
        .unwrap();
    ledger
        .publish_generation(completed_generation(gen1.clone()))
        .await
        .unwrap();

    let gen2 = ledger
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    ledger
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("README.md", "same")],
        ))
        .await
        .unwrap();
    ledger
        .publish_generation(completed_generation(gen2))
        .await
        .unwrap();

    assert_eq!(ledger.cleanup_debt_count().await, 1);
}

#[tokio::test]
async fn fake_ledger_acquires_conflicts_reclaims_and_releases_leases() {
    let ledger = FakeLedgerStore::new();

    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");
    let conflict = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-b"))
        .await
        .unwrap();
    assert_eq!(conflict, None);

    let wrong_owner_release = ledger
        .release_lease(first.lease_id.clone(), "owner-b".to_string())
        .await
        .unwrap_err();
    assert_eq!(
        wrong_owner_release.code.to_string(),
        "source.ledger.lease_owner_mismatch"
    );

    ledger
        .release_lease(first.lease_id.clone(), "owner-a".to_string())
        .await
        .unwrap();
    let expired = ledger
        .acquire_lease(lease_request_ttl("source:src_a:refresh", "owner-a", 0))
        .await
        .unwrap()
        .expect("zero ttl lease");
    let reclaimed = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-b"))
        .await
        .unwrap()
        .expect("expired lease should be reclaimable");
    assert_ne!(expired.lease_id, reclaimed.lease_id);
    assert_eq!(reclaimed.owner_id, "owner-b");

    let stale_heartbeat = ledger
        .heartbeat_lease(expired.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .unwrap();
    assert_eq!(stale_heartbeat, None);
    let stale_release = ledger
        .release_lease(expired.lease_id, "owner-a".to_string())
        .await
        .unwrap_err();
    assert_eq!(
        stale_release.code.to_string(),
        "source.ledger.lease_missing"
    );
    let owner_c_conflict = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-c"))
        .await
        .unwrap();
    assert_eq!(owner_c_conflict, None);

    ledger
        .release_lease(reclaimed.lease_id.clone(), "owner-b".to_string())
        .await
        .unwrap();
    let reacquired = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap();
    assert!(reacquired.is_some());
}

#[tokio::test]
async fn fake_ledger_same_owner_can_renew_lease() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");

    let renewed = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("same owner renewal");

    assert_eq!(renewed.lease_id, first.lease_id);
    assert_eq!(renewed.acquired_at, first.acquired_at);
}

#[tokio::test]
async fn fake_ledger_heartbeat_rejects_expired_lease() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request_ttl("source:src_a:refresh", "owner-a", 0))
        .await
        .unwrap()
        .expect("first lease");

    let heartbeat = ledger
        .heartbeat_lease(first.lease_id, "owner-a".to_string(), 30)
        .await
        .unwrap();

    assert_eq!(heartbeat, None);
}

#[tokio::test]
async fn fake_ledger_heartbeat_rejects_wrong_owner() {
    let ledger = FakeLedgerStore::new();
    let first = ledger
        .acquire_lease(lease_request("source:src_a:refresh", "owner-a"))
        .await
        .unwrap()
        .expect("first lease");

    let heartbeat = ledger
        .heartbeat_lease(first.lease_id, "owner-b".to_string(), 30)
        .await
        .unwrap();

    assert_eq!(heartbeat, None);
}

use axon_api::source::*;
use uuid::Uuid;

use crate::migration::migrate_ledger;
use crate::sqlite::SqliteLedgerStore;
use crate::store::LedgerStore;

fn ts() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn ts_at(second: u32) -> Timestamp {
    Timestamp(format!("2026-07-01T00:00:{second:02}Z"))
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src_sqlite"),
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
        tags: vec!["sqlite".to_string()],
        watch_id: None,
        last_job_id: None,
    }
}

fn manifest(hash: &str) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_sqlite"),
        generation: SourceGenerationId::new(format!("gen_{hash}")),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Directory,
        items: vec![manifest_item("src/lib.rs", hash)],
        created_at: ts(),
        metadata: MetadataMap::new(),
    }
}

fn manifest_with_items(generation: &str, items: Vec<ManifestItem>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new("src_sqlite"),
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
        source_id: SourceId::new("src_sqlite"),
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

fn completed_generation_for_manifest(manifest: &SourceManifest) -> SourceGeneration {
    SourceGeneration {
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        status: LifecycleStatus::Completed,
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

fn lease_request(key: &str, owner: &str, acquired_at: Timestamp) -> LeaseRequest {
    LeaseRequest {
        lease_key: key.to_string(),
        owner_id: owner.to_string(),
        ttl_seconds: 30,
        acquired_at,
        job_id: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn sqlite_source_round_trips() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let source = source();

    store
        .upsert_source(source.clone())
        .await
        .expect("upsert source");

    let stored = store
        .get_source(SourceId::new("src_sqlite"))
        .await
        .expect("get source");

    assert_eq!(stored, Some(source));
}

#[tokio::test]
async fn sqlite_diff_manifest_against_committed_generation() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let uncommitted = manifest("uncommitted");
    store
        .put_manifest(uncommitted)
        .await
        .expect("put uncommitted manifest");
    let no_previous = store.diff_manifest(manifest("next")).await.expect("diff");
    assert_eq!(no_previous.previous_generation, None);
    assert_eq!(no_previous.counts.added, 1);
    assert_eq!(no_previous.counts.modified, 0);

    let committed = manifest_with_items(
        "gen_committed",
        vec![
            manifest_item("src/lib.rs", "old"),
            manifest_item("README.md", "same"),
            manifest_item("src/old.rs", "removed"),
        ],
    );
    store
        .put_manifest(committed.clone())
        .await
        .expect("put committed manifest");
    store
        .publish_generation(completed_generation_for_manifest(&committed))
        .await
        .expect("publish committed manifest");

    let next = manifest_with_items(
        "gen_next",
        vec![
            manifest_item("src/lib.rs", "new"),
            manifest_item("README.md", "same"),
            manifest_item("src/main.rs", "added"),
        ],
    );
    let diff = store.diff_manifest(next).await.expect("diff committed");

    assert_eq!(
        diff.previous_generation,
        Some(SourceGenerationId::new("gen_committed"))
    );
    assert_eq!(diff.counts.added, 1);
    assert_eq!(diff.counts.modified, 1);
    assert_eq!(diff.counts.removed, 1);
    assert_eq!(diff.counts.unchanged, 1);
    assert_eq!(
        diff.added[0].source_item_key,
        SourceItemKey::new("src/main.rs")
    );
    assert_eq!(
        diff.modified[0].source_item_key,
        SourceItemKey::new("src/lib.rs")
    );
    assert_eq!(
        diff.removed[0].source_item_key,
        SourceItemKey::new("src/old.rs")
    );
    assert_eq!(
        diff.unchanged[0].source_item_key,
        SourceItemKey::new("README.md")
    );
}

#[tokio::test]
async fn sqlite_records_document_status_and_cleanup_debt_idempotently() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let status = DocumentStatus {
        document_id: DocumentId::new("doc-sqlite"),
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_1"),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts(),
        chunk_count: 2,
        vector_point_count: 2,
        error: None,
        cleanup_status: None,
    };
    store
        .update_document_status(status.clone())
        .await
        .expect("record document status");
    assert_eq!(
        store
            .document_status(&DocumentId::new("doc-sqlite"))
            .await
            .expect("read document status"),
        Some(status)
    );

    let debt = CleanupDebt {
        debt_id: CleanupDebtId::new("debt-sqlite"),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new("src_sqlite"),
        generation: Some(SourceGenerationId::new("gen_1")),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::Document {
            document_id: DocumentId::new("doc-sqlite"),
        },
        status: LifecycleStatus::Pending,
        created_at: ts(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    };
    store
        .record_cleanup_debt(debt.clone())
        .await
        .expect("record cleanup debt");
    store
        .record_cleanup_debt(debt)
        .await
        .expect("record cleanup debt idempotently");

    assert_eq!(
        store
            .cleanup_debt_count()
            .await
            .expect("count cleanup debt"),
        1
    );
}

#[tokio::test]
async fn sqlite_acquires_conflicts_reclaims_and_releases_leases() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let first = store
        .acquire_lease(lease_request(
            "source:src_sqlite:refresh",
            "owner-a",
            ts_at(0),
        ))
        .await
        .expect("acquire first")
        .expect("first lease");
    let conflict = store
        .acquire_lease(lease_request(
            "source:src_sqlite:refresh",
            "owner-b",
            ts_at(10),
        ))
        .await
        .expect("conflicting acquire");
    assert_eq!(conflict, None);

    let reclaimed = store
        .acquire_lease(lease_request(
            "source:src_sqlite:refresh",
            "owner-b",
            ts_at(31),
        ))
        .await
        .expect("reclaim expired")
        .expect("expired lease should be reclaimable");
    assert_ne!(first.lease_id, reclaimed.lease_id);
    assert_eq!(reclaimed.owner_id, "owner-b");

    store
        .release_lease(reclaimed.lease_id.clone())
        .await
        .expect("release lease");
    let reacquired = store
        .acquire_lease(lease_request(
            "source:src_sqlite:refresh",
            "owner-a",
            ts_at(32),
        ))
        .await
        .expect("reacquire after release");
    assert!(reacquired.is_some());
}

#[tokio::test]
async fn sqlite_migration_creates_required_ledger_tables() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    migrate_ledger(&pool).await.expect("migrate ledger");

    let tables = sqlx::query_scalar::<_, String>(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE type = 'table' AND name LIKE 'axon_ledger_%'
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("table names");

    assert_eq!(
        tables,
        vec![
            "axon_ledger_cleanup_debt",
            "axon_ledger_document_status",
            "axon_ledger_generations",
            "axon_ledger_leases",
            "axon_ledger_source_items",
            "axon_ledger_source_manifests",
            "axon_ledger_sources",
        ]
    );
}

#[tokio::test]
async fn sqlite_store_enables_foreign_keys() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    assert!(store.foreign_keys_enabled().await.expect("foreign keys"));
}

#[tokio::test]
async fn sqlite_generation_publish_controls_committed_baseline() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let running = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");
    assert_eq!(running.generation, SourceGenerationId::new("gen_1"));
    assert_eq!(running.previous_generation, None);

    let error = store
        .publish_generation(running.clone())
        .await
        .expect_err("running generation is not publishable");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_not_publishable"
    );

    let committed_manifest = manifest_with_items(
        &running.generation.0,
        vec![manifest_item("src/lib.rs", "committed")],
    );
    store
        .put_manifest(committed_manifest)
        .await
        .expect("put committed manifest");
    store
        .publish_generation(completed_generation_for_manifest(&manifest_with_items(
            &running.generation.0,
            vec![manifest_item("src/lib.rs", "committed")],
        )))
        .await
        .expect("publish completed generation");

    let next = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create next generation");
    assert_eq!(next.generation, SourceGenerationId::new("gen_2"));
    assert_eq!(
        next.previous_generation,
        Some(SourceGenerationId::new("gen_1"))
    );

    store
        .put_manifest(manifest_with_items(
            "gen_2",
            vec![manifest_item("src/lib.rs", "interrupted")],
        ))
        .await
        .expect("put interrupted manifest");
    let diff = store
        .diff_manifest(manifest_with_items(
            "gen_3",
            vec![manifest_item("src/lib.rs", "committed")],
        ))
        .await
        .expect("diff against committed generation");
    assert_eq!(diff.counts.unchanged, 1);
    assert_eq!(diff.counts.added, 0);
}

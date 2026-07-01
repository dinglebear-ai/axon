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
        publish_state: PublishState::Writing,
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

fn completed_generation_from(generation: &SourceGeneration) -> SourceGeneration {
    let mut generation = generation.clone();
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Writing;
    generation
}

fn publish_request(generation: &SourceGeneration) -> PublishGenerationRequest {
    PublishGenerationRequest {
        source_id: generation.source_id.clone(),
        generation: generation.generation.clone(),
        expected_previous_generation: generation.previous_generation.clone(),
    }
}

async fn complete_and_publish(
    store: &SqliteLedgerStore,
    generation: SourceGeneration,
) -> SourceGeneration {
    let completed = store
        .complete_generation(generation)
        .await
        .expect("complete generation");
    store
        .publish_generation(publish_request(&completed))
        .await
        .expect("publish generation")
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
async fn sqlite_generation_timestamps_are_runtime_values() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let generation = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");

    assert_ne!(generation.created_at, ts());
    assert!(
        chrono::DateTime::parse_from_rfc3339(&generation.created_at.0).is_ok(),
        "created_at should be RFC3339: {:?}",
        generation.created_at
    );
}

#[tokio::test]
async fn sqlite_scalar_status_columns_use_schema_wire_values() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/lib.rs", "same")],
        ))
        .await
        .expect("put manifest");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let generation_status: String =
        sqlx::query_scalar("SELECT status FROM source_generations WHERE generation = ?1")
            .bind(&gen1.generation.0)
            .fetch_one(&store.pool)
            .await
            .expect("read generation status");
    let publish_state: String =
        sqlx::query_scalar("SELECT publish_state FROM source_generations WHERE generation = ?1")
            .bind(&gen1.generation.0)
            .fetch_one(&store.pool)
            .await
            .expect("read generation publish_state");
    assert_eq!(generation_status, "completed");
    assert_eq!(publish_state, "committed");

    store
        .update_document_status(DocumentStatus {
            document_id: DocumentId::new("doc-sqlite"),
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            generation: gen1.generation.clone(),
            status: DocumentLifecycleStatus::Published,
            updated_at: ts(),
            chunk_count: 1,
            vector_point_count: 1,
            error: None,
            cleanup_status: None,
        })
        .await
        .expect("record document status");
    let document_status: String =
        sqlx::query_scalar("SELECT status FROM document_status WHERE document_id = ?1")
            .bind("doc-sqlite")
            .fetch_one(&store.pool)
            .await
            .expect("read document status");
    assert_eq!(document_status, "published");

    store
        .record_cleanup_debt(CleanupDebt {
            debt_id: CleanupDebtId::new("debt-sqlite"),
            job_id: JobId::new(Uuid::from_u128(1)),
            source_id: SourceId::new("src_sqlite"),
            generation: Some(gen1.generation),
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
        })
        .await
        .expect("record cleanup debt");
    let cleanup_status: String =
        sqlx::query_scalar("SELECT status FROM cleanup_debt WHERE debt_id = ?1")
            .bind("debt-sqlite")
            .fetch_one(&store.pool)
            .await
            .expect("read cleanup status");
    assert_eq!(cleanup_status, "pending");
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
    complete_and_publish(&store, completed_generation_for_manifest(&committed)).await;

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
async fn sqlite_rejects_invalid_manifest_item_ownership_and_duplicates() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let mut wrong_source = manifest("wrong-source");
    wrong_source.items[0].source_id = SourceId::new("other");
    let error = store
        .put_manifest(wrong_source)
        .await
        .expect_err("wrong source");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.manifest_item_source_mismatch"
    );

    let mut duplicate = manifest_with_items(
        "gen_duplicate",
        vec![
            manifest_item("src/lib.rs", "a"),
            manifest_item("src/lib.rs", "b"),
        ],
    );
    duplicate.items[1].canonical_uri = "file:///repo/src/lib-copy.rs".to_string();
    let error = store
        .put_manifest(duplicate)
        .await
        .expect_err("duplicate item key");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.manifest_duplicate_item"
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
    let updated = DocumentStatus {
        document_id: DocumentId::new("doc-sqlite"),
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_2"),
        status: DocumentLifecycleStatus::Failed,
        updated_at: ts_at(9),
        chunk_count: 0,
        vector_point_count: 0,
        error: Some(SourceError {
            code: "embed.failed".to_string(),
            severity: Severity::Failed,
            message: "embedding failed".to_string(),
            source_item_key: Some(SourceItemKey::new("src/lib.rs")),
            retryable: true,
            provider_id: None,
            cause: None,
        }),
        cleanup_status: Some(LifecycleStatus::Pending),
    };
    store
        .update_document_status(updated.clone())
        .await
        .expect("overwrite document status");
    assert_eq!(
        store
            .document_status(&DocumentId::new("doc-sqlite"))
            .await
            .expect("read overwritten document status"),
        Some(updated)
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
async fn sqlite_cleanup_debt_uses_natural_key_and_terminal_state_is_monotonic() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let mut debt = CleanupDebt {
        debt_id: CleanupDebtId::new("debt-a"),
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
        .expect("record debt");

    debt.debt_id = CleanupDebtId::new("debt-b");
    store
        .record_cleanup_debt(debt.clone())
        .await
        .expect("same selector is idempotent even with a new debt id");
    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);

    debt.status = LifecycleStatus::Completed;
    debt.completed_at = Some(ts_at(10));
    store
        .record_cleanup_debt(debt.clone())
        .await
        .expect("complete debt");

    debt.status = LifecycleStatus::Pending;
    debt.completed_at = None;
    store
        .record_cleanup_debt(debt)
        .await
        .expect("stale replay should not regress terminal debt");

    let stored = store
        .cleanup_debt(&CleanupDebtId::new("debt-b"))
        .await
        .expect("read debt")
        .expect("stored debt");
    assert_eq!(stored.status, LifecycleStatus::Completed);
    assert_eq!(stored.completed_at, Some(ts_at(10)));
}

#[tokio::test]
async fn sqlite_rejects_document_status_and_cleanup_debt_for_missing_sources() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let status_err = store
        .update_document_status(DocumentStatus {
            document_id: DocumentId::new("doc-missing"),
            source_id: SourceId::new("missing"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            generation: SourceGenerationId::new("gen_1"),
            status: DocumentLifecycleStatus::Published,
            updated_at: ts(),
            chunk_count: 1,
            vector_point_count: 1,
            error: None,
            cleanup_status: None,
        })
        .await
        .expect_err("missing source should reject document status");
    assert_eq!(status_err.code.to_string(), "source.ledger.sqlite");

    let debt_err = store
        .record_cleanup_debt(CleanupDebt {
            debt_id: CleanupDebtId::new("debt-missing"),
            job_id: JobId::new(Uuid::from_u128(1)),
            source_id: SourceId::new("missing"),
            generation: None,
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::Document {
                document_id: DocumentId::new("doc-missing"),
            },
            status: LifecycleStatus::Pending,
            created_at: ts(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        })
        .await
        .expect_err("missing source should reject cleanup debt");
    assert_eq!(debt_err.code.to_string(), "source.ledger.sqlite");
}

#[tokio::test]
async fn sqlite_rejects_cleanup_selector_source_or_generation_mismatch() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let base = CleanupDebt {
        debt_id: CleanupDebtId::new("debt-mismatch"),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new("src_sqlite"),
        generation: Some(SourceGenerationId::new("gen_1")),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::SourceItem {
            source_id: SourceId::new("other"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            generation: SourceGenerationId::new("gen_1"),
        },
        status: LifecycleStatus::Pending,
        created_at: ts(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    };
    let source_err = store
        .record_cleanup_debt(base.clone())
        .await
        .expect_err("selector source mismatch should fail");
    assert_eq!(
        source_err.code.to_string(),
        "source.ledger.cleanup_selector_mismatch"
    );

    let mut generation_mismatch = base;
    generation_mismatch.selector = CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_2"),
    };
    let generation_err = store
        .record_cleanup_debt(generation_mismatch)
        .await
        .expect_err("selector generation mismatch should fail");
    assert_eq!(
        generation_err.code.to_string(),
        "source.ledger.cleanup_selector_mismatch"
    );
}

#[tokio::test]
async fn sqlite_acquires_conflicts_reclaims_and_releases_leases() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire first")
        .expect("first lease");
    let conflict = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-b"))
        .await
        .expect("conflicting acquire");
    assert_eq!(conflict, None);

    let wrong_owner_release = store
        .release_lease(first.lease_id.clone(), "owner-b".to_string())
        .await
        .expect_err("wrong owner cannot release an active lease");
    assert_eq!(
        wrong_owner_release.code.to_string(),
        "source.ledger.lease_owner_mismatch"
    );

    store
        .release_lease(first.lease_id.clone(), "owner-a".to_string())
        .await
        .expect("release first lease");
    let expired = store
        .acquire_lease(lease_request_ttl("source:src_sqlite:refresh", "owner-a", 0))
        .await
        .expect("acquire expired lease")
        .expect("zero ttl lease");
    let reclaimed = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-b"))
        .await
        .expect("reclaim expired")
        .expect("expired lease should be reclaimable");
    assert_ne!(expired.lease_id, reclaimed.lease_id);
    assert_eq!(reclaimed.owner_id, "owner-b");

    let stale_heartbeat = store
        .heartbeat_lease(expired.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .expect("heartbeat with stale guard");
    assert_eq!(stale_heartbeat, None);
    let stale_release = store
        .release_lease(expired.lease_id, "owner-a".to_string())
        .await
        .expect_err("stale guard cannot release reclaimed lease");
    assert_eq!(
        stale_release.code.to_string(),
        "source.ledger.lease_missing"
    );
    let owner_c_conflict = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-c"))
        .await
        .expect("owner-c conflict");
    assert_eq!(owner_c_conflict, None);

    store
        .release_lease(reclaimed.lease_id.clone(), "owner-b".to_string())
        .await
        .expect("release lease");
    let reacquired = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("reacquire after release");
    assert!(reacquired.is_some());
}

#[tokio::test]
async fn sqlite_same_owner_can_renew_lease() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let renewed = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("renew")
        .expect("renewed lease");

    assert_eq!(renewed.lease_id, first.lease_id);
    assert_eq!(renewed.acquired_at, first.acquired_at);
}

#[tokio::test]
async fn sqlite_heartbeat_extends_lease_by_id() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .expect("heartbeat")
        .expect("lease should still exist");

    assert_eq!(heartbeat.lease_id, first.lease_id);
    assert_eq!(heartbeat.owner_id, "owner-a");
    assert!(chrono::DateTime::parse_from_rfc3339(&heartbeat.heartbeat_at.0).is_ok());
}

#[tokio::test]
async fn sqlite_heartbeat_rejects_expired_lease() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request_ttl("source:src_sqlite:refresh", "owner-a", 0))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id, "owner-a".to_string(), 30)
        .await
        .expect("heartbeat");

    assert_eq!(heartbeat, None);
}

#[tokio::test]
async fn sqlite_heartbeat_rejects_wrong_owner() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id, "owner-b".to_string(), 30)
        .await
        .expect("heartbeat");

    assert_eq!(heartbeat, None);
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
        WHERE type = 'table'
          AND name IN (
            'sources',
            'source_generations',
            'source_items',
            'source_manifests',
            'document_status',
            'cleanup_debt',
            'leases'
          )
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("table names");

    assert_eq!(
        tables,
        vec![
            "cleanup_debt",
            "document_status",
            "leases",
            "source_generations",
            "source_items",
            "source_manifests",
            "sources",
        ]
    );
}

#[tokio::test]
async fn sqlite_store_enables_foreign_keys() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    assert!(store.foreign_keys_enabled().await.expect("foreign keys"));
}

#[tokio::test]
async fn sqlite_generation_sequence_is_unique_per_source() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    let duplicate = sqlx::query(
        r#"
        INSERT INTO source_generations (
            source_id,
            generation,
            sequence,
            status,
            publish_state,
            generation_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind("src_sqlite")
    .bind("gen_duplicate")
    .bind(1_i64)
    .bind("running")
    .bind("writing")
    .bind("{}")
    .bind(ts().0)
    .execute(&store.pool)
    .await;

    assert!(
        duplicate.is_err(),
        "duplicate sequence for {:?} should violate the unique index",
        gen1.generation
    );
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
        .publish_generation(publish_request(&running))
        .await
        .expect_err("running generation is not publishable");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_not_publishable"
    );

    let missing_manifest = completed_generation_from(&running);
    let error = store
        .complete_generation(missing_manifest)
        .await
        .expect_err("completed generation without manifest is not publishable");
    assert_eq!(error.code.to_string(), "source.ledger.manifest_missing");

    let committed_manifest = manifest_with_items(
        &running.generation.0,
        vec![manifest_item("src/lib.rs", "committed")],
    );
    store
        .put_manifest(committed_manifest)
        .await
        .expect("put committed manifest");
    let published = complete_and_publish(
        &store,
        completed_generation_for_manifest(&manifest_with_items(
            &running.generation.0,
            vec![manifest_item("src/lib.rs", "committed")],
        )),
    )
    .await;
    assert_eq!(published.publish_state, PublishState::Committed);
    assert!(published.published_at.is_some());

    let generation_row: (String, String) = sqlx::query_as(
        "SELECT publish_state, generation_json FROM source_generations WHERE generation = ?1",
    )
    .bind(&running.generation.0)
    .fetch_one(&store.pool)
    .await
    .expect("read stored published generation");
    assert_eq!(generation_row.0, "committed");
    let stored_generation: SourceGeneration =
        serde_json::from_str(&generation_row.1).expect("parse generation json");
    assert_eq!(stored_generation.publish_state, PublishState::Committed);
    assert!(stored_generation.published_at.is_some());

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

#[tokio::test]
async fn sqlite_publish_rejects_stale_generation_baseline() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/lib.rs", "gen1")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let stale = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create stale gen2");
    let fresh = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create fresh gen3");
    store
        .put_manifest(manifest_with_items(
            &fresh.generation.0,
            vec![manifest_item("src/lib.rs", "gen3")],
        ))
        .await
        .expect("put fresh");
    complete_and_publish(&store, completed_generation_from(&fresh)).await;

    store
        .put_manifest(manifest_with_items(
            &stale.generation.0,
            vec![manifest_item("src/lib.rs", "gen2")],
        ))
        .await
        .expect("put stale");
    let completed_stale = store
        .complete_generation(completed_generation_from(&stale))
        .await
        .expect("complete stale generation");
    let error = store
        .publish_generation(publish_request(&completed_stale))
        .await
        .expect_err("stale generation cannot rewind committed baseline");
    assert_eq!(
        error.code.to_string(),
        "source.ledger.generation_baseline_changed"
    );

    let diff = store
        .diff_manifest(manifest_with_items(
            "gen_next",
            vec![manifest_item("src/lib.rs", "gen3")],
        ))
        .await
        .expect("diff");
    assert_eq!(diff.previous_generation, Some(fresh.generation));
    assert_eq!(diff.counts.unchanged, 1);
}

#[tokio::test]
async fn sqlite_publish_creates_cleanup_debt_for_removed_items() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![
                manifest_item("README.md", "same"),
                manifest_item("src/old.rs", "removed"),
            ],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("README.md", "same")],
        ))
        .await
        .expect("put gen2");
    let published = complete_and_publish(&store, completed_generation_from(&gen2)).await;

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);
    let debt_json: String = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_one(&store.pool)
        .await
        .expect("read cleanup debt");
    let debt: CleanupDebt = serde_json::from_str(&debt_json).expect("parse cleanup debt");
    assert_eq!(debt.kind, CleanupDebtKind::VectorDelete);
    assert_eq!(debt.generation, Some(gen1.generation.clone()));
    assert_eq!(
        debt.selector,
        CleanupSelector::SourceItem {
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/old.rs"),
            generation: gen1.generation,
        }
    );
    let generation_json: String =
        sqlx::query_scalar("SELECT generation_json FROM source_generations WHERE generation = ?1")
            .bind(&gen2.generation.0)
            .fetch_one(&store.pool)
            .await
            .expect("read generation json");
    let stored_generation: SourceGeneration =
        serde_json::from_str(&generation_json).expect("parse generation json");
    assert_eq!(stored_generation.cleanup_debt, vec![debt.debt_id]);
    assert_eq!(
        stored_generation.publish_state,
        PublishState::CleanupPending
    );
    assert_eq!(published.publish_state, PublishState::CleanupPending);
}

#[tokio::test]
async fn sqlite_publish_creates_cleanup_debt_for_modified_items() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/lib.rs", "old")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(
            &gen2.generation.0,
            vec![manifest_item("src/lib.rs", "new")],
        ))
        .await
        .expect("put gen2");
    complete_and_publish(&store, completed_generation_from(&gen2)).await;

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);
    let debt_json: String = sqlx::query_scalar("SELECT debt_json FROM cleanup_debt")
        .fetch_one(&store.pool)
        .await
        .expect("read cleanup debt");
    let debt: CleanupDebt = serde_json::from_str(&debt_json).expect("parse cleanup debt");
    assert_eq!(
        debt.selector,
        CleanupSelector::SourceItem {
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/lib.rs"),
            generation: gen1.generation,
        }
    );
}

#[tokio::test]
async fn sqlite_publish_keeps_distinct_cleanup_debt_for_readded_item_generations() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen1");
    store
        .put_manifest(manifest_with_items(
            &gen1.generation.0,
            vec![manifest_item("src/old.rs", "first")],
        ))
        .await
        .expect("put gen1");
    complete_and_publish(&store, completed_generation_from(&gen1)).await;

    let gen2 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen2");
    store
        .put_manifest(manifest_with_items(&gen2.generation.0, vec![]))
        .await
        .expect("put gen2");
    complete_and_publish(&store, completed_generation_from(&gen2)).await;

    let gen3 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen3");
    store
        .put_manifest(manifest_with_items(
            &gen3.generation.0,
            vec![manifest_item("src/old.rs", "second")],
        ))
        .await
        .expect("put gen3");
    complete_and_publish(&store, completed_generation_from(&gen3)).await;

    let gen4 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create gen4");
    store
        .put_manifest(manifest_with_items(&gen4.generation.0, vec![]))
        .await
        .expect("put gen4");
    complete_and_publish(&store, completed_generation_from(&gen4)).await;

    assert_eq!(store.cleanup_debt_count().await.expect("count"), 2);
    let rows = sqlx::query_scalar::<_, String>("SELECT debt_json FROM cleanup_debt")
        .fetch_all(&store.pool)
        .await
        .expect("read cleanup debt");
    let selectors = rows
        .into_iter()
        .map(|json| {
            let debt: CleanupDebt = serde_json::from_str(&json).expect("parse cleanup debt");
            debt.selector
        })
        .collect::<Vec<_>>();
    assert!(selectors.contains(&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen1.generation,
    }));
    assert!(selectors.contains(&CleanupSelector::SourceItem {
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/old.rs"),
        generation: gen3.generation,
    }));
}

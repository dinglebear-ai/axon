use super::*;
use uuid::Uuid;

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
async fn sqlite_create_generation_rejects_missing_source_with_domain_error() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let err = store
        .create_generation(SourceId::new("missing-source"))
        .await
        .expect_err("missing source should fail");

    assert_eq!(err.code.to_string(), "source.ledger.source_missing");
}

#[tokio::test]
async fn sqlite_document_status_requires_existing_source_item() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");
    let gen1 = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");

    let err = store
        .update_document_status(DocumentStatus {
            document_id: DocumentId::new("doc-missing-item"),
            source_id: SourceId::new("src_sqlite"),
            source_item_key: SourceItemKey::new("src/missing.rs"),
            generation: gen1.generation,
            status: DocumentLifecycleStatus::Published,
            updated_at: ts(),
            chunk_count: 1,
            vector_point_count: 1,
            error: None,
            cleanup_status: None,
        })
        .await
        .expect_err("missing source item should fail");

    assert_eq!(err.code.to_string(), "source.ledger.source_item_missing");
}

#[tokio::test]
async fn sqlite_cleanup_debt_requires_existing_generation() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let err = store
        .record_cleanup_debt(CleanupDebt {
            debt_id: CleanupDebtId::new("debt-missing-generation"),
            job_id: JobId::new(Uuid::from_u128(1)),
            source_id: SourceId::new("src_sqlite"),
            generation: Some(SourceGenerationId::new("gen_missing")),
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::Generation {
                source_id: SourceId::new("src_sqlite"),
                generation: SourceGenerationId::new("gen_missing"),
            },
            status: LifecycleStatus::Pending,
            created_at: ts(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        })
        .await
        .expect_err("missing generation should fail");

    assert_eq!(err.code.to_string(), "source.ledger.generation_missing");
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
async fn sqlite_source_manifest_requires_existing_generation() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let orphan = manifest("orphan");
    let orphan_json = serde_json::to_string(&orphan).expect("manifest json");
    let orphan_result = sqlx::query(
        r#"
        INSERT INTO source_manifests (
            source_id,
            generation,
            manifest_json,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&orphan.source_id.0)
    .bind(&orphan.generation.0)
    .bind(orphan_json)
    .bind(&orphan.created_at.0)
    .execute(&store.pool)
    .await;
    assert!(
        orphan_result.is_err(),
        "manifest without a source_generation row should fail"
    );

    let generation = store
        .create_generation(SourceId::new("src_sqlite"))
        .await
        .expect("create generation");
    let valid = manifest_with_items(
        &generation.generation.0,
        vec![manifest_item("src/lib.rs", "valid")],
    );
    store
        .put_manifest(valid)
        .await
        .expect("valid generation manifest");
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

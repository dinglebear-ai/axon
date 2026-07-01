use super::*;
use uuid::Uuid;

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
async fn sqlite_document_status_ignores_stale_updates() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    store.upsert_source(source()).await.expect("upsert source");

    let newer = DocumentStatus {
        document_id: DocumentId::new("doc-sqlite"),
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_2"),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts_at(9),
        chunk_count: 2,
        vector_point_count: 2,
        error: None,
        cleanup_status: None,
    };
    store
        .update_document_status(newer.clone())
        .await
        .expect("record newer status");

    let stale = DocumentStatus {
        document_id: DocumentId::new("doc-sqlite"),
        source_id: SourceId::new("src_sqlite"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_1"),
        status: DocumentLifecycleStatus::Failed,
        updated_at: ts(),
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
        .update_document_status(stale)
        .await
        .expect("stale status replay");

    assert_eq!(
        store
            .document_status(&DocumentId::new("doc-sqlite"))
            .await
            .expect("read document status"),
        Some(newer)
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
async fn sqlite_cleanup_debt_ignores_stale_replay() {
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
    debt.job_id = JobId::new(Uuid::from_u128(2));
    debt.status = LifecycleStatus::Failed;
    debt.created_at = ts_at(9);
    debt.attempts = 3;
    debt.last_error = Some(SourceError {
        code: "cleanup.failed".to_string(),
        severity: Severity::Failed,
        message: "cleanup failed".to_string(),
        source_item_key: Some(SourceItemKey::new("src/lib.rs")),
        retryable: true,
        provider_id: None,
        cause: None,
    });
    debt.next_retry_at = Some(ts_at(30));
    let newer = debt.clone();
    store
        .record_cleanup_debt(newer.clone())
        .await
        .expect("record newer debt");

    let mut stale = debt;
    stale.debt_id = CleanupDebtId::new("debt-c");
    stale.job_id = JobId::new(Uuid::from_u128(3));
    stale.status = LifecycleStatus::Pending;
    stale.created_at = ts_at(1);
    stale.attempts = 1;
    stale.last_error = None;
    stale.next_retry_at = None;
    store
        .record_cleanup_debt(stale)
        .await
        .expect("stale debt replay");

    assert_eq!(
        store
            .cleanup_debt(&CleanupDebtId::new("debt-b"))
            .await
            .expect("read debt"),
        Some(newer)
    );
    assert_eq!(store.cleanup_debt_count().await.expect("count"), 1);
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
    assert_eq!(status_err.code.to_string(), "source.ledger.source_missing");

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
    assert_eq!(debt_err.code.to_string(), "source.ledger.source_missing");
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

use super::*;
use uuid::Uuid;

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
    let updated = DocumentStatus {
        document_id: DocumentId::new("doc-a"),
        source_id: SourceId::new("src_a"),
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
    ledger
        .update_document_status(updated.clone())
        .await
        .unwrap();
    assert_eq!(
        ledger.document_status(&DocumentId::new("doc-a")).await,
        Some(updated)
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
async fn fake_rejects_document_status_and_cleanup_debt_for_missing_sources() {
    let ledger = FakeLedgerStore::new();

    let status_err = ledger
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
        .unwrap_err();
    assert_eq!(status_err.code.to_string(), "source.ledger.source_missing");

    let debt_err = ledger
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
        .unwrap_err();
    assert_eq!(debt_err.code.to_string(), "source.ledger.source_missing");
}

#[tokio::test]
async fn fake_rejects_cleanup_selector_source_or_generation_mismatch() {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();

    let base = CleanupDebt {
        debt_id: CleanupDebtId::new("debt-mismatch"),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new("src_a"),
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
    let source_err = ledger.record_cleanup_debt(base.clone()).await.unwrap_err();
    assert_eq!(
        source_err.code.to_string(),
        "source.ledger.cleanup_selector_mismatch"
    );

    let mut generation_mismatch = base;
    generation_mismatch.selector = CleanupSelector::SourceItem {
        source_id: SourceId::new("src_a"),
        source_item_key: SourceItemKey::new("src/lib.rs"),
        generation: SourceGenerationId::new("gen_2"),
    };
    let generation_err = ledger
        .record_cleanup_debt(generation_mismatch)
        .await
        .unwrap_err();
    assert_eq!(
        generation_err.code.to_string(),
        "source.ledger.cleanup_selector_mismatch"
    );
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
    complete_and_publish(&ledger, completed_generation(gen1.clone())).await;

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
    let published = complete_and_publish(&ledger, completed_generation(gen2)).await;

    assert_eq!(ledger.cleanup_debt_count().await, 1);
    assert_eq!(published.publish_state, PublishState::CleanupPending);
    assert_eq!(published.cleanup_debt.len(), 1);
}

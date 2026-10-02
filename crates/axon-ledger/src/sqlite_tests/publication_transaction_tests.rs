use super::*;
use axon_core::sqlite::ImmediateTx;

async fn publication_fixture() -> (
    SqliteLedgerStore,
    SourceGeneration,
    SourceGeneration,
    DocumentStatus,
) {
    let store = SqliteLedgerStore::connect("sqlite::memory:").await.unwrap();
    let id = source().source_id;
    store.upsert_source(source()).await.unwrap();
    let first = store.create_generation(id.clone()).await.unwrap();
    let mut inventory = manifest("one");
    inventory.generation = first.generation.clone();
    inventory
        .items
        .push(manifest_item("src/removed.rs", "removed"));
    store.put_manifest(inventory.clone()).await.unwrap();
    let first = store
        .complete_generation(completed_generation_from(&first))
        .await
        .unwrap();
    store
        .publish_generation(publish_request(&first))
        .await
        .unwrap();
    let status = DocumentStatus {
        document_id: DocumentId::new("retained-document"),
        source_id: id.clone(),
        source_item_key: inventory.items[0].source_item_key.clone(),
        generation: Some(first.generation.clone()),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts(),
        chunk_count: 1,
        vector_point_count: 1,
        error: None,
        cleanup_status: None,
    };
    store.update_document_status(status.clone()).await.unwrap();
    let next = store.create_generation(id.clone()).await.unwrap();
    inventory.generation = next.generation.clone();
    inventory.items.pop();
    store.put_manifest(inventory).await.unwrap();
    let next = store
        .complete_generation(completed_generation_from(&next))
        .await
        .unwrap();
    (store, first, next, status)
}

#[tokio::test]
async fn caller_owned_publication_can_rollback_or_commit_all_ledger_effects() {
    let (store, first, next, status) = publication_fixture().await;
    let id = first.source_id.clone();
    let mut request = publish_request(&next);
    request.retained_statuses = vec![status.clone()];
    let debt_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cleanup_debt")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    let mut tx = ImmediateTx::begin_with_gate(&store.pool, &store.write_gate)
        .await
        .unwrap();
    let published = store
        .publish_generation_in_tx(&mut tx, request.clone())
        .await
        .unwrap();
    assert!(
        !published.cleanup_debt.is_empty(),
        "replacement creates retirement debt"
    );
    tx.rollback().await;
    assert_eq!(
        store.committed_generation(id.clone()).await.unwrap(),
        Some(first.generation.clone())
    );
    assert_eq!(
        store
            .document_statuses_for_items(id.clone(), vec![status.source_item_key.clone()])
            .await
            .unwrap(),
        vec![status.clone()]
    );
    let debt_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cleanup_debt")
        .fetch_one(store.pool_for_tests())
        .await
        .unwrap();
    assert_eq!(debt_after, debt_before);
    let stored_json: String = sqlx::query_scalar(
        "SELECT generation_json FROM source_generations WHERE source_id = ? AND generation = ?",
    )
    .bind(&id.0)
    .bind(&next.generation.0)
    .fetch_one(store.pool_for_tests())
    .await
    .unwrap();
    let stored: SourceGeneration = serde_json::from_str(&stored_json).unwrap();
    assert_eq!(
        stored, next,
        "rollback restores completed unpublished generation"
    );

    let epoch: i64 = sqlx::query_scalar(
        "SELECT committed_epoch FROM source_publication_state WHERE source_id = ?",
    )
    .bind(&id.0)
    .fetch_one(store.pool_for_tests())
    .await
    .unwrap();
    assert_eq!(epoch, 1);
    let mut tx = ImmediateTx::begin_with_gate(&store.pool, &store.write_gate)
        .await
        .unwrap();
    let mut stale = request.clone();
    stale.expected_previous_generation = None;
    assert_eq!(
        store
            .publish_generation_in_tx(&mut tx, stale)
            .await
            .unwrap_err()
            .code,
        "source.ledger.generation_baseline_changed".into()
    );
    tx.rollback().await;
    let mut tx = ImmediateTx::begin_with_gate(&store.pool, &store.write_gate)
        .await
        .unwrap();
    let committed = store
        .publish_generation_in_tx(&mut tx, request)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(committed.publish_state, published.publish_state);
    assert_eq!(committed.cleanup_debt, published.cleanup_debt);
    assert_eq!(
        store.committed_generation(id.clone()).await.unwrap(),
        Some(next.generation.clone())
    );
    let rows = store
        .document_statuses_for_items(id.clone(), vec![status.source_item_key])
        .await
        .unwrap();
    assert_eq!(rows[0].generation, Some(next.generation));
    let epoch: i64 = sqlx::query_scalar(
        "SELECT committed_epoch FROM source_publication_state WHERE source_id = ?",
    )
    .bind(&id.0)
    .fetch_one(store.pool_for_tests())
    .await
    .unwrap();
    assert_eq!(epoch, 2);
}

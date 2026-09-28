use super::*;

async fn exercise(store: &dyn LedgerStore) {
    let source = source();
    let id = source.source_id.clone();
    store.upsert_source(source).await.unwrap();
    let first = store.create_generation(id.clone()).await.unwrap();
    let mut inventory = manifest("one");
    inventory.generation = first.generation.clone();
    store.put_manifest(inventory.clone()).await.unwrap();
    let first = store
        .complete_generation(completed_generation_from(&first))
        .await
        .unwrap();
    store
        .publish_generation(publish_request(&first))
        .await
        .unwrap();
    let next = store.create_generation(id.clone()).await.unwrap();
    inventory.generation = next.generation.clone();
    store.put_manifest(inventory.clone()).await.unwrap();
    let key = inventory.items[0].source_item_key.clone();
    let base = DocumentStatus {
        document_id: DocumentId::new("doc-one"),
        source_id: id.clone(),
        source_item_key: key.clone(),
        generation: Some(first.generation.clone()),
        status: DocumentLifecycleStatus::Published,
        updated_at: ts(),
        chunk_count: 100,
        vector_point_count: 100,
        error: None,
        cleanup_status: None,
    };
    let mut sibling = base.clone();
    sibling.document_id = DocumentId::new("doc-two");
    sibling.status = DocumentLifecycleStatus::Skipped;
    sibling.chunk_count = 0;
    sibling.vector_point_count = 0;
    store
        .update_document_statuses(vec![base.clone(), sibling.clone()])
        .await
        .unwrap();
    let expected = store
        .document_statuses_for_items(id.clone(), vec![key.clone(), key.clone()])
        .await
        .unwrap();
    assert_eq!(expected.len(), 2);
    assert!(
        store
            .carry_document_statuses(
                id.clone(),
                first.generation.clone(),
                next.generation.clone(),
                vec![base.clone()],
                ts()
            )
            .await
            .is_err(),
        "extra sibling must reject carry"
    );
    let attempt = || {
        store.carry_document_statuses(
            id.clone(),
            first.generation.clone(),
            next.generation.clone(),
            expected.clone(),
            ts(),
        )
    };
    let (left, right) = tokio::join!(attempt(), attempt());
    assert_eq!(
        usize::from(left.is_ok()) + usize::from(right.is_ok()),
        1,
        "only one concurrent carry can consume the expected snapshot"
    );
    assert_eq!(left.or(right).unwrap(), 2);
    let carried = store
        .document_statuses_for_items(id.clone(), vec![key.clone()])
        .await
        .unwrap();
    assert!(
        carried
            .iter()
            .all(|s| s.generation.as_ref() == Some(&next.generation))
    );
    assert_eq!(carried[1].status, DocumentLifecycleStatus::Skipped);
    assert_eq!(
        carried
            .iter()
            .map(|s| u64::from(s.chunk_count))
            .sum::<u64>(),
        100
    );
    assert!(
        store
            .carry_document_statuses(
                id.clone(),
                first.generation.clone(),
                next.generation.clone(),
                vec![base.clone(), sibling.clone()],
                ts()
            )
            .await
            .is_err(),
        "stale snapshot cannot clobber carried rows"
    );
    let mut overwritten = carried[0].clone();
    overwritten.status = DocumentLifecycleStatus::Failed;
    store.update_document_status(overwritten).await.unwrap();
    let actual = store
        .document_statuses_for_items(id, vec![key])
        .await
        .unwrap();
    assert_eq!(
        actual[0].status,
        DocumentLifecycleStatus::Failed,
        "lookup must reveal noncommitted latest rows"
    );
}

#[tokio::test]
async fn sqlite_status_carry_checks_provenance_and_siblings() {
    let store = SqliteLedgerStore::connect("sqlite::memory:").await.unwrap();
    exercise(&store).await;
    let rows = sqlx::query("EXPLAIN QUERY PLAN SELECT status_json FROM document_status WHERE source_id = ? AND source_item_key IN (?)")
        .bind("src").bind("item").fetch_all(store.pool_for_tests()).await.unwrap();
    use sqlx::Row;
    assert!(rows.iter().any(|r| {
        r.get::<String, _>("detail")
            .contains("idx_document_status_source_item")
    }));
}

#[tokio::test]
async fn fake_status_carry_checks_provenance_and_siblings() {
    exercise(&crate::store::FakeLedgerStore::default()).await;
}

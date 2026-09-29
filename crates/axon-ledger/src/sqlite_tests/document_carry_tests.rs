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

async fn failed_publish_then_partial_retry(
    store: &dyn LedgerStore,
    sqlite: Option<&SqliteLedgerStore>,
) {
    let source = source();
    let id = source.source_id.clone();
    store.upsert_source(source).await.unwrap();
    let first = store.create_generation(id.clone()).await.unwrap();
    let items = vec![
        manifest_item("src/one.rs", "one"),
        manifest_item("src/two.rs", "two"),
    ];
    let mut inventory = manifest_with_items(&first.generation.0, items);
    store.put_manifest(inventory.clone()).await.unwrap();
    let first = store
        .complete_generation(completed_generation_from(&first))
        .await
        .unwrap();
    store
        .publish_generation(publish_request(&first))
        .await
        .unwrap();

    let statuses = inventory
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| DocumentStatus {
            document_id: DocumentId::new(format!("doc-{index}")),
            source_id: id.clone(),
            source_item_key: item.source_item_key.clone(),
            generation: Some(first.generation.clone()),
            status: DocumentLifecycleStatus::Published,
            updated_at: ts(),
            chunk_count: 1,
            vector_point_count: 1,
            error: None,
            cleanup_status: None,
        })
        .collect::<Vec<_>>();
    store
        .update_document_statuses(statuses.clone())
        .await
        .unwrap();

    let next = store.create_generation(id.clone()).await.unwrap();
    inventory.generation = next.generation.clone();
    store.put_manifest(inventory).await.unwrap();
    let next = store
        .complete_generation(completed_generation_from(&next))
        .await
        .unwrap();
    let mut invalid = publish_request(&next);
    invalid.retained_statuses = statuses.clone();
    if let Some(sqlite) = sqlite {
        sqlx::query("CREATE TRIGGER fail_publish_after_carry BEFORE UPDATE OF committed_generation ON sources BEGIN SELECT RAISE(ABORT, 'injected publish failure'); END")
            .execute(sqlite.pool_for_tests()).await.unwrap();
    } else {
        invalid.expected_previous_generation = None;
    }
    assert!(store.publish_generation(invalid).await.is_err());
    if let Some(sqlite) = sqlite {
        sqlx::query("DROP TRIGGER fail_publish_after_carry")
            .execute(sqlite.pool_for_tests())
            .await
            .unwrap();
    }
    assert_eq!(
        store.committed_generation(id.clone()).await.unwrap(),
        Some(first.generation.clone())
    );
    assert_eq!(
        store
            .document_statuses_for_items(
                id.clone(),
                vec![
                    statuses[0].source_item_key.clone(),
                    statuses[1].source_item_key.clone()
                ]
            )
            .await
            .unwrap(),
        statuses
    );

    let mut changed = statuses[1].clone();
    changed.generation = Some(next.generation.clone());
    changed.updated_at = ts_at(1);
    changed.status = DocumentLifecycleStatus::Prepared;
    store.update_document_status(changed.clone()).await.unwrap();
    let mut retry = publish_request(&next);
    retry.retained_statuses = vec![statuses[0].clone()];
    store.publish_generation(retry).await.unwrap();
    let rows = store
        .document_statuses_for_items(
            id.clone(),
            vec![
                statuses[0].source_item_key.clone(),
                statuses[1].source_item_key.clone(),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        store.committed_generation(id).await.unwrap(),
        Some(next.generation.clone())
    );
    assert_eq!(rows[0].generation, Some(next.generation.clone()));
    assert_eq!(rows[1], changed);
}

#[tokio::test]
async fn sqlite_failed_publish_preserves_statuses_for_partial_retry() {
    let store = SqliteLedgerStore::connect("sqlite::memory:").await.unwrap();
    failed_publish_then_partial_retry(&store, Some(&store)).await;
}

#[tokio::test]
async fn fake_failed_publish_preserves_statuses_for_partial_retry() {
    failed_publish_then_partial_retry(&crate::store::FakeLedgerStore::default(), None).await;
}

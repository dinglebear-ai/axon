use super::*;

async fn publish_items(store: &dyn LedgerStore, hash: &str, skip: bool) -> SourceGeneration {
    let generation = store
        .create_generation(SourceId::new("src_a"))
        .await
        .unwrap();
    store
        .put_manifest(manifest_with_items(
            &generation.generation.0,
            vec![manifest_item("file", hash)],
        ))
        .await
        .unwrap();
    store
        .update_document_status(DocumentStatus {
            document_id: DocumentId::new("doc"),
            source_id: SourceId::new("src_a"),
            source_item_key: SourceItemKey::new("file"),
            generation: Some(generation.generation.clone()),
            status: if skip {
                DocumentLifecycleStatus::Skipped
            } else {
                DocumentLifecycleStatus::Published
            },
            updated_at: ts(),
            chunk_count: u32::from(!skip),
            vector_point_count: u32::from(!skip),
            error: None,
            cleanup_status: None,
        })
        .await
        .unwrap();
    let completed = store
        .complete_generation(completed_generation(generation))
        .await
        .unwrap();
    store
        .publish_generation(publish_request(&completed))
        .await
        .unwrap()
}

#[tokio::test]
async fn fake_and_sqlite_record_graph_retirement_for_present_skipped_identity() {
    let stores: Vec<Box<dyn LedgerStore>> = vec![
        Box::new(FakeLedgerStore::new()),
        Box::new(SqliteLedgerStore::in_memory().await.unwrap()),
    ];
    for store in stores {
        store.upsert_source(source()).await.unwrap();
        let first = publish_items(store.as_ref(), "text", false).await;
        let skipped = publish_items(store.as_ref(), "binary", true).await;
        let debts = store
            .list_pending_cleanup_debt(SourceId::new("src_a"))
            .await
            .unwrap();
        let debt = debts
            .iter()
            .find(|debt| debt.kind == CleanupDebtKind::GraphPrune)
            .expect("present skipped item must retire old graph output");
        assert_eq!(debt.generation, Some(first.generation));
        assert_eq!(
            debt.selector,
            CleanupSelector::GraphItemEvidence {
                source_id: SourceId::new("src_a"),
                source_item_key: SourceItemKey::new("file"),
                retirement_generation: skipped.generation.clone()
            }
        );
        let retained = store
            .get_manifest(SourceId::new("src_a"), skipped.generation)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.items.len(), 1, "skip preserves inventory");
    }
}

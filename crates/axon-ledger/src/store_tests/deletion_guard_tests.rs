use super::*;

#[tokio::test]
async fn generation_delete_preserves_pending_nonledger_dependencies_atomically() {
    let stores: Vec<Box<dyn LedgerStore>> = vec![
        Box::new(FakeLedgerStore::new()),
        Box::new(SqliteLedgerStore::in_memory().await.unwrap()),
    ];
    for store in stores {
        store.upsert_source(source()).await.unwrap();
        let old = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        store
            .put_manifest(manifest_with_items(&old.generation.0, vec![]))
            .await
            .unwrap();
        let old = store
            .complete_generation(completed_generation(old))
            .await
            .unwrap();
        store
            .publish_generation(publish_request(&old))
            .await
            .unwrap();
        let current = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        store
            .put_manifest(manifest_with_items(&current.generation.0, vec![]))
            .await
            .unwrap();
        let current = store
            .complete_generation(completed_generation(current))
            .await
            .unwrap();
        store
            .publish_generation(publish_request(&current))
            .await
            .unwrap();
        let debt = crate::cleanup_debt::generation_vector_delete_debt(
            &SourceId::new("src_a"),
            &old.generation,
        );
        let id = debt.debt_id.clone();
        store.record_cleanup_debt(debt).await.unwrap();
        let error = store
            .delete_generation(SourceId::new("src_a"), old.generation.clone())
            .await
            .unwrap_err();
        assert_eq!(error.code.0, "source.ledger.generation_cleanup_pending");
        assert!(
            store
                .get_manifest(SourceId::new("src_a"), old.generation.clone())
                .await
                .unwrap()
                .is_some()
        );
        store.resolve_cleanup_debt(id).await.unwrap();
        // Other generations' debt and this generation's ledger debt do not block it.
        store
            .record_cleanup_debt(crate::cleanup_debt::generation_vector_delete_debt(
                &SourceId::new("src_a"),
                &current.generation,
            ))
            .await
            .unwrap();
        store
            .record_cleanup_debt(crate::cleanup_debt::ledger_prune_debt(
                &SourceId::new("src_a"),
                &old.generation,
            ))
            .await
            .unwrap();
        assert!(
            store
                .delete_generation(SourceId::new("src_a"), old.generation.clone())
                .await
                .unwrap()
                > 0
        );
        assert!(
            store
                .get_manifest(SourceId::new("src_a"), old.generation)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .get_manifest(SourceId::new("src_a"), current.generation)
                .await
                .unwrap()
                .is_some()
        );
    }
}

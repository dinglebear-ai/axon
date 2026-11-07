use super::*;

#[tokio::test]
async fn recovery_preserves_published_current_newer_and_same_lease_writes() {
    let stores: Vec<Box<dyn LedgerStore>> = vec![
        Box::new(FakeLedgerStore::new()),
        Box::new(SqliteLedgerStore::in_memory().await.unwrap()),
    ];
    for store in stores {
        store.upsert_source(source()).await.unwrap();
        let published = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        store
            .put_manifest(manifest_with_items(&published.generation.0, vec![]))
            .await
            .unwrap();
        let complete = store
            .complete_generation(completed_generation(published.clone()))
            .await
            .unwrap();
        store
            .publish_generation(publish_request(&complete))
            .await
            .unwrap();
        // Unknown imported generations must not consume the bounded recovery page.
        for _ in 0..65 {
            let collision = store
                .create_generation(SourceId::new("src_a"))
                .await
                .unwrap();
            store.fail_generation(collision).await.unwrap();
        }
        let abandoned = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        let mut manifest = manifest_with_items(&abandoned.generation.0, vec![]);
        manifest.metadata.insert(
            crate::GENERATION_VECTOR_COLLECTION_METADATA_KEY.into(),
            "old-collection".into(),
        );
        store.put_manifest(manifest).await.unwrap();
        store.fail_generation(abandoned.clone()).await.unwrap();
        let imported = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        store.fail_generation(imported.clone()).await.unwrap();
        let lease = store
            .acquire_lease(LeaseRequest {
                lease_key: "source:src_a".into(),
                owner_id: "worker".into(),
                ttl_seconds: 60,
                job_id: None,
                metadata: MetadataMap::new(),
            })
            .await
            .unwrap()
            .unwrap();
        let same_lease = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        let current = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        let newer = store
            .create_generation(SourceId::new("src_a"))
            .await
            .unwrap();
        let mut forged = current.clone();
        forged.created_at = Timestamp("2099-01-01T00:00:00Z".into());
        assert_eq!(
            store
                .recover_abandoned_generations(forged, lease.clone(), "axon".into())
                .await
                .unwrap(),
            1
        );
        let debts = store
            .list_pending_cleanup_debt(SourceId::new("src_a"))
            .await
            .unwrap();
        assert_eq!(debts.len(), 1);
        assert_eq!(
            debts[0].vector_collection.as_deref(),
            Some("old-collection")
        );
        assert_eq!(debts[0].generation.as_ref(), Some(&abandoned.generation));
        for protected in [&published, &imported, &same_lease, &current, &newer] {
            assert!(
                !debts
                    .iter()
                    .any(|d| d.generation.as_ref() == Some(&protected.generation))
            );
        }
        assert_eq!(
            store
                .committed_generation(SourceId::new("src_a"))
                .await
                .unwrap(),
            Some(published.generation)
        );
        store
            .release_lease(lease.lease_id.clone(), lease.owner_id.clone())
            .await
            .unwrap();
        assert!(
            store
                .recover_abandoned_generations(current, lease, "axon".into())
                .await
                .is_err()
        );
    }
}

use super::*;
use axon_api::source::{CleanupDebt, CleanupDebtId, CleanupDebtKind, CleanupSelector};
use axon_prune::PruneTarget;

async fn fixture() -> (Arc<FakeLedgerStore>, SourceGenerationId) {
    let (ledger, _) = ledger_with_committed_generation("owner/repo").await;
    let generation = ledger
        .create_generation(SourceId::new("owner/repo"))
        .await
        .unwrap()
        .generation;
    ledger
        .put_manifest(ledger_manifest("owner/repo", &generation))
        .await
        .unwrap();
    for (id, collection, kind) in [
        ("matching", "axon-test", CleanupDebtKind::VectorDelete),
        ("other-collection", "other", CleanupDebtKind::VectorDelete),
        ("graph", "axon-test", CleanupDebtKind::GraphPrune),
    ] {
        ledger
            .record_cleanup_debt(CleanupDebt {
                debt_id: CleanupDebtId::new(id),
                job_id: JobId::new(Uuid::from_u128(1)),
                origin_attempt: 0,
                source_id: SourceId::new("owner/repo"),
                generation: Some(generation.clone()),
                kind,
                selector: if id == "other-collection" {
                    CleanupSelector::SourceItem {
                        source_id: SourceId::new("owner/repo"),
                        source_item_key: SourceItemKey::new("other.rs"),
                        generation: generation.clone(),
                    }
                } else {
                    CleanupSelector::Generation {
                        source_id: SourceId::new("owner/repo"),
                        generation: generation.clone(),
                    }
                },
                vector_collection: Some(collection.to_string()),
                status: LifecycleStatus::Pending,
                created_at: Timestamp::from(chrono::Utc::now()),
                attempts: 0,
                last_error: None,
                next_retry_at: None,
                completed_at: None,
            })
            .await
            .unwrap();
    }
    (Arc::new(ledger), generation)
}

fn step(generation: SourceGenerationId) -> PruneStep {
    PruneStep {
        target: PruneTargetKind::Vector,
        description: "delete generation vectors".to_string(),
        estimated_deletes: 0,
        vector_selector: Some(VectorDeleteSelector::Generation {
            collection: "axon-test".to_string(),
            source_id: SourceId::new("owner/repo"),
            generation: generation.clone(),
        }),
        source_id: Some(SourceId::new("owner/repo")),
        generation: Some(generation),
        graph_stable_keys: None,
        graph_edge_ids: None,
        memory_ids: None,
    }
}

#[tokio::test]
async fn successful_manual_vector_prune_resolves_only_covered_debt() {
    let (ledger, generation) = fixture().await;
    let vector = FakeVectorStore::new("fake");
    vector
        .ensure_collection(test_collection_spec(3))
        .await
        .unwrap();
    let target = VectorOnlyPruneTarget::with_ledger(&vector, "axon-test", ledger.clone());
    target.apply(&step(generation.clone())).await.unwrap();
    let pending = ledger
        .list_pending_cleanup_debt(SourceId::new("owner/repo"))
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|debt| debt.debt_id.0 != "matching"));
    // Unrelated provider and collection debt still protect recovery metadata.
    let mut ledger_step = step(generation.clone());
    ledger_step.target = PruneTargetKind::Ledger;
    assert!(target.apply(&ledger_step).await.is_err());
    for debt in pending {
        ledger.resolve_cleanup_debt(debt.debt_id).await.unwrap();
    }
    target.apply(&ledger_step).await.unwrap();
    assert!(
        ledger
            .get_manifest(SourceId::new("owner/repo"), generation)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn failed_manual_vector_prune_preserves_all_debt() {
    let (ledger, generation) = fixture().await;
    let target =
        VectorOnlyPruneTarget::with_ledger(&FailingVectorStore, "axon-test", ledger.clone());
    assert!(target.apply(&step(generation.clone())).await.is_err());
    assert_eq!(
        ledger
            .list_pending_cleanup_debt(SourceId::new("owner/repo"))
            .await
            .unwrap()
            .len(),
        3
    );
    assert!(
        ledger
            .get_manifest(SourceId::new("owner/repo"), generation)
            .await
            .unwrap()
            .is_some()
    );
}

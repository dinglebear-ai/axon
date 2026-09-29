use super::*;
use axon_api::source::{
    DocumentId, DocumentLifecycleStatus, DocumentStatus, LeaseRequest, PublishGenerationRequest,
    PublishState,
};
use axon_graph::{FakeGraphStore, GraphStore};

async fn publish(ledger: &FakeLedgerStore, present: bool, skipped: bool) -> SourceGenerationId {
    let mut generation = ledger
        .create_generation(SourceId::new("src"))
        .await
        .unwrap();
    let items = if present {
        vec![manifest_item(
            "src",
            "file",
            "file:///file",
            ItemKind::LocalFile,
        )]
    } else {
        vec![]
    };
    ledger
        .put_manifest(manifest("src", &generation.generation.0, items))
        .await
        .unwrap();
    if present {
        ledger
            .update_document_status(DocumentStatus {
                document_id: DocumentId::new("doc"),
                source_id: SourceId::new("src"),
                source_item_key: SourceItemKey::new("file"),
                generation: Some(generation.generation.clone()),
                status: if skipped {
                    DocumentLifecycleStatus::Skipped
                } else {
                    DocumentLifecycleStatus::Published
                },
                updated_at: Timestamp(chrono::Utc::now().to_rfc3339()),
                chunk_count: u32::from(!skipped),
                vector_point_count: u32::from(!skipped),
                error: None,
                cleanup_status: None,
            })
            .await
            .unwrap();
    }
    generation.status = LifecycleStatus::Completed;
    generation.publish_state = PublishState::Writing;
    ledger
        .complete_generation(generation.clone())
        .await
        .unwrap();
    ledger
        .publish_generation(PublishGenerationRequest {
            job_id: JobId::new(Uuid::nil()),
            attempt: 0,
            source_id: generation.source_id,
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation,
            retained_statuses: Vec::new(),
        })
        .await
        .unwrap();
    generation.generation
}

async fn baseline(
    graph: &FakeGraphStore,
    ledger: &FakeLedgerStore,
    generation: &SourceGenerationId,
) {
    let manifest = ledger
        .get_manifest(SourceId::new("src"), generation.clone())
        .await
        .unwrap()
        .unwrap();
    let counts = counts("src", &generation.0);
    graph
        .upsert_candidates(
            baseline_candidates(SourceKind::Local, &counts, "file:///root", &manifest).collect(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn graph_retirement_waits_for_source_lease_and_handles_later_generations() {
    let ledger = Arc::new(FakeLedgerStore::new());
    ledger
        .upsert_source(source_summary("src", "file:///root"))
        .await
        .unwrap();
    let graph = Arc::new(FakeGraphStore::new());
    let first = publish(&ledger, true, false).await;
    baseline(&graph, &ledger, &first).await;
    let skipped = publish(&ledger, true, true).await;
    let held = ledger
        .acquire_lease(LeaseRequest {
            lease_key: "source:src".into(),
            owner_id: "refresh".into(),
            ttl_seconds: 1800,
            job_id: None,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap()
        .unwrap();
    let retire = || {
        super::super::lease::retire_under_lease(
            ledger.clone(),
            SourceId::new("src"),
            SourceItemKey::new("file"),
            skipped.clone(),
            JobId::new(Uuid::nil()),
            |source, item| graph.retire_item_evidence(source, item),
        )
    };
    assert!(
        retire().await.is_err(),
        "concurrent refresh owns source identity"
    );
    assert_eq!(
        graph
            .nodes_for_source(SourceId::new("src"))
            .await
            .unwrap()
            .len(),
        2
    );
    ledger
        .release_lease(held.lease_id, held.owner_id)
        .await
        .unwrap();
    publish(&ledger, false, false).await;
    assert_eq!(
        retire().await.unwrap().nodes_deleted,
        1,
        "old debt still cleans absent item after unrelated publication"
    );
    let regained = publish(&ledger, true, false).await;
    baseline(&graph, &ledger, &regained).await;
    assert_eq!(
        retire().await.unwrap().nodes_deleted,
        0,
        "retry preserves reintroduced supported text"
    );
    assert_eq!(
        graph
            .nodes_for_source(SourceId::new("src"))
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn graph_publication_filters_skips_and_rejects_failed_generation_status() {
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(source_summary("src", "file:///root"))
        .await
        .unwrap();
    let skipped = publish(&ledger, true, true).await;
    let mut current = ledger
        .get_manifest(SourceId::new("src"), skipped.clone())
        .await
        .unwrap()
        .unwrap();
    super::super::lease::filter_skipped(&ledger, &mut current)
        .await
        .unwrap();
    assert!(current.items.is_empty());
    let mut status = ledger
        .document_statuses_for_items(SourceId::new("src"), vec![SourceItemKey::new("file")])
        .await
        .unwrap()
        .remove(0);
    status.generation = Some(SourceGenerationId::new("failed-refresh"));
    status.status = DocumentLifecycleStatus::Failed;
    status.updated_at = Timestamp("2099-01-01T00:00:00Z".into());
    ledger.update_document_status(status).await.unwrap();
    let mut current = ledger
        .get_manifest(SourceId::new("src"), skipped)
        .await
        .unwrap()
        .unwrap();
    assert!(
        super::super::lease::filter_skipped(&ledger, &mut current)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn same_generation_ineligible_status_defers_graph_publication_and_retirement() {
    for disposition in [
        DocumentLifecycleStatus::Failed,
        DocumentLifecycleStatus::Cleaned,
        DocumentLifecycleStatus::Discovered,
    ] {
        let ledger = Arc::new(FakeLedgerStore::new());
        ledger
            .upsert_source(source_summary("src", "file:///root"))
            .await
            .unwrap();
        let graph = Arc::new(FakeGraphStore::new());
        let generation = publish(&ledger, true, false).await;
        baseline(&graph, &ledger, &generation).await;
        let mut status = ledger
            .document_statuses_for_items(SourceId::new("src"), vec![SourceItemKey::new("file")])
            .await
            .unwrap()
            .remove(0);
        status.status = disposition;
        status.updated_at = Timestamp("2099-01-01T00:00:00Z".into());
        ledger.update_document_status(status).await.unwrap();
        let mut current = ledger
            .get_manifest(SourceId::new("src"), generation.clone())
            .await
            .unwrap()
            .unwrap();
        let error = super::super::lease::filter_skipped(ledger.as_ref(), &mut current)
            .await
            .unwrap_err();
        assert_eq!(error.code.to_string(), "graph.publication_status_unknown");
        let error = super::super::lease::retire_under_lease(
            ledger.clone(),
            SourceId::new("src"),
            SourceItemKey::new("file"),
            generation,
            JobId::new(Uuid::nil()),
            |source, item| graph.retire_item_evidence(source, item),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code.to_string(), "graph.retirement_status_unknown");
        assert_eq!(
            graph
                .nodes_for_source(SourceId::new("src"))
                .await
                .unwrap()
                .len(),
            2
        );
    }
}

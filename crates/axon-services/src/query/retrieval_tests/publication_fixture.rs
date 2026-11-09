use axon_api::source::*;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("src-docs"),
        canonical_uri: "file:///fixture".into(),
        display_name: "fixture".into(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".into(),
            version: "test".into(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            documents_skipped: 0,
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: Timestamp("2026-10-16T00:00:00Z".into()),
        updated_at: Timestamp("2026-10-16T00:00:00Z".into()),
        watch_id: None,
        graph_node_ids: vec![],
        last_job_id: None,
        last_refreshed_at: None,
        tags: vec![],
        user_label: None,
    }
}
async fn publish(ledger: &FakeLedgerStore) -> SourceGenerationId {
    let mut generation = ledger
        .create_generation(SourceId::new("src-docs"))
        .await
        .unwrap();
    ledger
        .put_manifest(SourceManifest {
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            adapter: source().adapter,
            scope: SourceScope::Repo,
            items: vec![],
            created_at: source().created_at,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    generation.status = LifecycleStatus::Completed;
    let generation = ledger.complete_generation(generation).await.unwrap();
    ledger
        .publish_generation(PublishGenerationRequest {
            job_id: JobId::new(uuid::Uuid::new_v4()),
            attempt: 1,
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation.clone(),
            retained_statuses: vec![],
        })
        .await
        .unwrap()
        .generation
}
pub(super) async fn ledger_at_seven() -> FakeLedgerStore {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    for _ in 0..7 {
        publish(&ledger).await;
    }
    ledger
}

use super::*;
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobId, JobIntent, JobKind, JobPriority,
    MetadataMap, SourceProgressEvent, SourceScope,
};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

async fn event_store_with_job() -> (Arc<FakeJobWatchStore>, JobId) {
    let store = Arc::new(FakeJobWatchStore::new());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Run,
            source_id: Some(SourceId::new("src-web-progress-test")),
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: Vec::new(),
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: Some(ConfigSnapshotId::new("cfg-web-progress-test")),
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .expect("create web progress event job");
    (store, descriptor.job_id)
}

#[tokio::test]
async fn failed_web_status_write_emits_an_uncounted_running_event() {
    let failed_status_store = Arc::new(FakeJobWatchStore::new());
    let (event_store, job_id) = event_store_with_job().await;
    let coordinator = WebProgressCoordinator {
        jobs: Some(failed_status_store),
        job_id,
        source_id: SourceId::new("src-web-progress-test"),
        attempt: 1,
    };
    let emitter = SourceEventEmitter::for_web(Some(event_store.clone()), job_id, SourceScope::Site)
        .with_source(
            SourceId::new("src-web-progress-test"),
            "https://example.test",
        );

    coordinator
        .report(
            &emitter,
            PipelinePhase::Embedding,
            stage_counts(Some(2), 2, Some(2), 2, Some(10), 3),
            "embedding web chunks",
        )
        .await;

    let events = event_store.recorded_events(job_id).await;
    assert_eq!(events.len(), 1);
    let progress: SourceProgressEvent = serde_json::from_value(
        events[0]
            .details
            .get("source_progress_event")
            .cloned()
            .expect("source progress payload"),
    )
    .expect("deserialize source progress event");
    assert_eq!(progress.phase, PipelinePhase::Embedding);
    assert_eq!(progress.status, LifecycleStatus::Running);
    assert_eq!(progress.counts, stage_counts(None, 0, None, 0, None, 0));
}

#[test]
fn downstream_totals_stay_unknown_until_the_final_web_batch() {
    let mut progress = WebPipelineProgress::new(2);

    assert_eq!(progress.fetch_start().items_total, Some(2));
    progress.acquired(1, 1);
    let first_normalized = progress.normalized(1, false);
    let first_prepared = progress.prepared(1, 350, false);
    let first_batched = progress.batched(350);
    let first_embedded = progress.embedded(350);
    let first_vectorized = progress.vectorized(250, false);

    for counts in [
        first_normalized,
        first_prepared,
        first_batched,
        first_embedded,
        first_vectorized,
    ] {
        assert_eq!(counts.documents_total, None);
        assert_eq!(counts.chunks_total, None);
    }

    progress.acquired(1, 1);
    let final_normalized = progress.normalized(1, true);
    let final_prepared = progress.prepared(1, 350, true);
    let final_batched = progress.batched(350);
    let final_embedded = progress.embedded(350);
    let final_vectorized = progress.vectorized(250, true);
    let final_upserted = progress.upserted(500);

    assert_eq!(final_normalized.documents_total, Some(2));
    assert_eq!(final_prepared.documents_total, Some(2));
    assert_eq!(final_prepared.chunks_total, Some(700));
    assert_eq!(final_batched.chunks_total, Some(700));
    assert_eq!(final_embedded.chunks_total, Some(700));
    assert_eq!(final_vectorized.chunks_total, Some(500));
    assert_eq!(final_upserted.chunks_total, Some(500));
    assert_eq!(final_upserted.chunks_done, 500);
}

#[test]
fn web_progress_counts_never_exceed_known_totals() {
    let mut progress = WebPipelineProgress::new(1);
    progress.acquired(4, 4);
    let normalized = progress.normalized(4, true);
    let prepared = progress.prepared(4, 10, true);
    let vectorized = progress.vectorized(12, true);
    let upserted = progress.upserted(20);

    assert_eq!(normalized.items_done, 1);
    assert_eq!(normalized.documents_done, 1);
    assert_eq!(prepared.documents_done, 1);
    assert_eq!(prepared.chunks_done, 10);
    assert_eq!(vectorized.chunks_total, Some(12));
    assert_eq!(vectorized.chunks_done, 12);
    assert_eq!(upserted.chunks_done, 12);
}

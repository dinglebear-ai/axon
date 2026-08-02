use super::*;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

#[derive(Default)]
struct RecordingWriter {
    updates: Mutex<Vec<JobStatusUpdate>>,
    fail: bool,
}

#[async_trait]
impl ProgressStatusWriter for RecordingWriter {
    async fn update(&self, update: JobStatusUpdate) -> JobResult<()> {
        self.updates.lock().await.push(update);
        if self.fail {
            return Err(ApiError::new(
                "test.progress_write_failed",
                ErrorStage::Observing,
                "synthetic progress persistence failure",
            ));
        }
        Ok(())
    }
}

fn coordinator(writer: Arc<RecordingWriter>) -> ProgressCoordinator {
    ProgressCoordinator::with_writer(
        writer,
        JobId::new(uuid::Uuid::from_u128(1)),
        SourceId::new("src-progress-test"),
        "local",
        Duration::ZERO,
    )
}

async fn event_store_with_job() -> (Arc<FakeJobWatchStore>, JobId) {
    let store = Arc::new(FakeJobWatchStore::new());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Run,
            source_id: Some(SourceId::new("src-progress-test")),
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: Vec::new(),
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: Some(ConfigSnapshotId::new("cfg-progress-test")),
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .expect("create progress event job");
    (store, descriptor.job_id)
}

fn event_emitter(store: Arc<FakeJobWatchStore>, job_id: JobId) -> SourceEventEmitter {
    SourceEventEmitter::new(Some(store), Some(job_id))
        .with_route(
            SourceKind::Local,
            SourceScope::Site,
            AdapterRef {
                name: "local".to_string(),
                version: "test".to_string(),
            },
        )
        .with_source(SourceId::new("src-progress-test"), "file:///progress-test")
}

#[tokio::test]
async fn runner_owned_batch_completion_publishes_global_counts_without_adapter_reports() {
    let writer = Arc::new(RecordingWriter::default());
    let coordinator = coordinator(writer.clone());
    coordinator
        .checkpoint(
            PipelinePhase::Fetching,
            stage_counts(Some(130), 0, Some(130), 0, None, 0),
            "starting acquisition",
        )
        .await;

    coordinator
        .acquisition_batch(130, 64, 0, 0)
        .complete(64)
        .await;
    coordinator
        .acquisition_batch(130, 64, 64, 64)
        .complete(63)
        .await;
    coordinator
        .acquisition_batch(130, 2, 128, 127)
        .complete(2)
        .await;

    let updates = writer.updates.lock().await;
    let snapshots = updates
        .iter()
        .map(|update| update.counts.as_ref().unwrap())
        .map(|counts| (counts.items_done, counts.documents_done))
        .collect::<Vec<_>>();
    assert_eq!(snapshots, vec![(0, 0), (64, 64), (128, 127), (130, 129)]);
}

#[tokio::test]
async fn malformed_and_regressing_adapter_snapshots_are_clamped_monotonically() {
    let writer = Arc::new(RecordingWriter::default());
    let coordinator = coordinator(writer.clone());
    let batch = coordinator.acquisition_batch(10, 5, 0, 0);

    batch
        .report(AcquisitionProgress {
            items_total: 99,
            items_done: 4,
            documents_done: 8,
        })
        .await;
    batch
        .report(AcquisitionProgress {
            items_total: 5,
            items_done: 2,
            documents_done: 1,
        })
        .await;
    batch
        .report(AcquisitionProgress {
            items_total: 5,
            items_done: 50,
            documents_done: 50,
        })
        .await;

    let updates = writer.updates.lock().await;
    let snapshots = updates
        .iter()
        .map(|update| update.counts.as_ref().unwrap())
        .map(|counts| (counts.items_done, counts.documents_done))
        .collect::<Vec<_>>();
    assert_eq!(snapshots, vec![(4, 4), (4, 4), (5, 5)]);
}

#[tokio::test]
async fn progress_persistence_failure_is_non_fatal() {
    let writer = Arc::new(RecordingWriter {
        updates: Mutex::new(Vec::new()),
        fail: true,
    });
    let coordinator = coordinator(writer.clone());

    coordinator
        .checkpoint(
            PipelinePhase::Embedding,
            stage_counts(Some(2), 2, Some(2), 2, Some(10), 3),
            "embedding chunks",
        )
        .await;

    assert_eq!(writer.updates.lock().await.len(), 1);
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Embedding)
            .await
            .unwrap()
            .chunks_done,
        3
    );
}

#[tokio::test]
async fn failed_status_write_emits_an_uncounted_running_event() {
    let writer = Arc::new(RecordingWriter {
        updates: Mutex::new(Vec::new()),
        fail: true,
    });
    let (store, job_id) = event_store_with_job().await;
    let coordinator = ProgressCoordinator::with_writer(
        writer,
        job_id,
        SourceId::new("src-progress-test"),
        "local",
        Duration::ZERO,
    );
    let emitter = event_emitter(store.clone(), job_id);

    coordinator
        .report(
            &emitter,
            PipelinePhase::Embedding,
            stage_counts(Some(2), 2, Some(2), 2, Some(10), 3),
            "embedding chunks",
        )
        .await;

    let events = store.recorded_events(job_id).await;
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
fn phase_totals_freeze_at_the_first_known_value() {
    let mut prior = Vec::new();
    let first = normalize_phase_counts(
        &mut prior,
        PipelinePhase::Embedding,
        stage_counts(Some(2), 2, Some(2), 2, Some(10), 3),
    );
    let expanded = normalize_phase_counts(
        &mut prior,
        PipelinePhase::Embedding,
        stage_counts(Some(4), 4, Some(4), 4, Some(20), 15),
    );
    let omitted = normalize_phase_counts(
        &mut prior,
        PipelinePhase::Embedding,
        stage_counts(None, 1, None, 1, None, 1),
    );

    assert_eq!(first.chunks_total, Some(10));
    assert_eq!(expanded.items_total, Some(2));
    assert_eq!(expanded.documents_total, Some(2));
    assert_eq!(expanded.chunks_total, Some(10));
    assert_eq!(expanded.items_done, 2);
    assert_eq!(expanded.documents_done, 2);
    assert_eq!(expanded.chunks_done, 10);
    assert_eq!(omitted, expanded);
}

#[test]
fn downstream_totals_remain_unknown_until_the_final_bounded_batch() {
    let mut progress = PipelineProgress::default();
    progress.add_documents(1);

    let first_preparing = progress.preparing_counts();
    let first_prepared = progress.prepared(1, 350, false);
    let first_batching = progress.batched(350);
    let first_embedding = progress.embedded(350);
    let first_vectorizing = progress.vectorized(250, false);

    for counts in [
        first_preparing,
        first_prepared,
        first_batching,
        first_embedding,
        first_vectorizing,
    ] {
        assert_eq!(counts.documents_total, None);
        assert_eq!(counts.chunks_total, None);
    }

    progress.add_documents(1);
    progress.finish_documents();
    let final_preparing = progress.preparing_counts();
    let final_prepared = progress.prepared(1, 350, true);
    let final_batching = progress.batched(162);
    let final_embedding = progress.embedded(162);
    let final_vectorizing = progress.vectorized(250, true);
    let final_upserting = progress.upserted(500);

    assert_eq!(final_preparing.documents_total, Some(2));
    assert_eq!(final_prepared.documents_total, Some(2));
    assert_eq!(final_prepared.chunks_total, Some(700));
    assert_eq!(final_batching.chunks_total, Some(700));
    assert_eq!(final_embedding.chunks_total, Some(700));
    assert_eq!(final_vectorizing.chunks_total, Some(500));
    assert_eq!(final_upserting.chunks_total, Some(500));
    assert_eq!(final_upserting.chunks_done, 500);
}

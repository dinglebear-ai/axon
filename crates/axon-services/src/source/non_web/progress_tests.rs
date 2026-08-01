use super::*;

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

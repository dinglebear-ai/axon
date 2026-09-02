use super::*;
use async_trait::async_trait;
use axon_adapters::boundary::FakeAdapterProviders;
use axon_adapters::{FakeSourceAdapter, SourceAdapter, web::WebSourceAdapter};
use axon_core::boundary::{ArtifactStore, FakeCoreBoundaries};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, mpsc, oneshot};

async fn controlled<T>(
    started: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _ = started.send(());
    let _ = release.await;
    result
}

#[tokio::test]
async fn collection_setup_overlaps_first_acquisition() {
    let (setup_started_tx, setup_started_rx) = oneshot::channel();
    let (setup_release_tx, setup_release_rx) = oneshot::channel();
    let (acquire_started_tx, acquire_started_rx) = oneshot::channel();
    let (acquire_release_tx, acquire_release_rx) = oneshot::channel();

    let joined = tokio::spawn(join_collection_setup_and_first_acquisition(
        controlled(setup_started_tx, setup_release_rx, Ok(())),
        controlled(acquire_started_tx, acquire_release_rx, Ok(42_u64)),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(1), setup_started_rx)
        .await
        .expect("collection setup must start")
        .expect("collection setup start signal");
    tokio::time::timeout(std::time::Duration::from_secs(1), acquire_started_rx)
        .await
        .expect("first acquisition must start before collection setup completes")
        .expect("first acquisition start signal");

    setup_release_tx.send(()).expect("release collection setup");
    acquire_release_tx
        .send(())
        .expect("release first acquisition");
    assert_eq!(joined.await.expect("join task").expect("joined calls"), 42);
}

#[derive(Clone, Copy, Default)]
struct AdapterFailures {
    acquire_call: Option<usize>,
    normalize_call: Option<usize>,
}

struct ControlledBatchAdapter {
    inner: FakeSourceAdapter,
    acquire_calls: AtomicUsize,
    normalize_calls: AtomicUsize,
    acquire_started: mpsc::UnboundedSender<usize>,
    normalize_started: mpsc::UnboundedSender<usize>,
    first_normalize_release: Mutex<Option<oneshot::Receiver<()>>>,
    second_acquire_release: Mutex<Option<oneshot::Receiver<()>>>,
    failures: AdapterFailures,
    prefetched_artifact: Option<ArtifactRef>,
}

impl ControlledBatchAdapter {
    fn new(
        item_count: usize,
        acquire_started: mpsc::UnboundedSender<usize>,
        normalize_started: mpsc::UnboundedSender<usize>,
        first_normalize_release: Option<oneshot::Receiver<()>>,
        failures: AdapterFailures,
    ) -> Self {
        let mut inner = FakeSourceAdapter::new(AdapterRef {
            name: "web".into(),
            version: "test".into(),
        });
        for index in 0..item_count {
            inner = inner.with_item(
                format!("item-{index:03}"),
                ContentKind::Markdown,
                format!("# Item {index}\nbody\n"),
            );
        }
        Self {
            inner,
            acquire_calls: AtomicUsize::new(0),
            normalize_calls: AtomicUsize::new(0),
            acquire_started,
            normalize_started,
            first_normalize_release: Mutex::new(first_normalize_release),
            second_acquire_release: Mutex::new(None),
            failures,
            prefetched_artifact: None,
        }
    }

    fn with_second_acquire_release(mut self, release: oneshot::Receiver<()>) -> Self {
        self.second_acquire_release = Mutex::new(Some(release));
        self
    }

    fn with_prefetched_artifact(mut self, artifact: ArtifactRef) -> Self {
        self.prefetched_artifact = Some(artifact);
        self
    }

    fn error(stage: ErrorStage, message: impl Into<String>) -> ApiError {
        ApiError::new("adapter.controlled.failure", stage, message)
    }
}

#[async_trait]
impl SourceAdapter for ControlledBatchAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn version(&self) -> &'static str {
        self.inner.version()
    }

    async fn capabilities(&self) -> axon_adapters::adapter::Result<SourceAdapterCapability> {
        self.inner.capabilities().await
    }

    async fn discover(&self, plan: &SourcePlan) -> axon_adapters::adapter::Result<SourceManifest> {
        self.inner.discover(plan).await
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> axon_adapters::adapter::Result<SourceAcquisition> {
        let call = self.acquire_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.acquire_started.send(call);
        if call == 2
            && let Some(release) = self.second_acquire_release.lock().await.take()
        {
            let _ = release.await;
        }
        if self.failures.acquire_call == Some(call) {
            return Err(Self::error(
                ErrorStage::Fetching,
                format!("acquire {call} failed"),
            ));
        }
        let mut acquisition = self.inner.acquire(plan, diff).await?;
        if call == 2
            && let Some(artifact) = &self.prefetched_artifact
        {
            acquisition.artifacts.push(artifact.clone());
        }
        Ok(acquisition)
    }

    fn supports_acquisition_prefetch(&self) -> bool {
        true
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> axon_adapters::adapter::Result<StageExecutionResult<Vec<SourceDocument>>> {
        let call = self.normalize_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.normalize_started.send(call);
        if call == 1
            && let Some(release) = self.first_normalize_release.lock().await.take()
        {
            let _ = release.await;
        }
        if self.failures.normalize_call == Some(call) {
            return Err(Self::error(
                ErrorStage::Normalizing,
                format!("normalize {call} failed"),
            ));
        }
        self.inner.normalize(plan, acquisition).await
    }
}

async fn run_actual_generation_batches(
    adapter: Arc<ControlledBatchAdapter>,
    artifact_store: Option<Arc<dyn ArtifactStore>>,
    keep_cleanup_armed: bool,
) -> (
    anyhow::Result<()>,
    GenerationStageProgress,
    ProgressCoordinator,
) {
    run_actual_generation_batches_with_diff(adapter, artifact_store, keep_cleanup_armed, |_| {})
        .await
}

async fn run_actual_generation_batches_with_diff(
    adapter: Arc<ControlledBatchAdapter>,
    artifact_store: Option<Arc<dyn ArtifactStore>>,
    keep_cleanup_armed: bool,
    mutate_diff: impl FnOnce(&mut SourceManifestDiff),
) -> (
    anyhow::Result<()>,
    GenerationStageProgress,
    ProgressCoordinator,
) {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ledger = Arc::new(FakeLedgerStore::new());
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors.clone(),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    if let Some(artifact_store) = artifact_store {
        runtime.artifact_store = artifact_store;
    }
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/overlap".to_string(),
    ))
    .expect("web route")
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        false,
        None,
        None,
    );
    let execution =
        crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
    let input = SourcePipelineInput {
        adapter: adapter.as_ref(),
        plan,
        collection: "overlap-test",
        owner_id: "overlap-test",
        auth_snapshot: None,
        execution: &execution,
    };
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            &input,
            LifecycleStatus::Running,
            empty_source_counts(),
            None,
        ))
        .await
        .expect("source summary");
    let manifest = adapter.discover(&input.plan).await.expect("manifest");
    let mut diff = runtime
        .ledger
        .diff_manifest(manifest)
        .await
        .expect("manifest diff");
    mutate_diff(&mut diff);
    let changed_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    let generation = diff.next_generation.clone();
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    let coordinator = ProgressCoordinator::test_noop();
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    let mut stage = GenerationStageProgress::default();
    let mut accumulated = GenerationAccumulator::default();
    let mut cleanup = ArtifactCleanupGuard::new(
        &runtime,
        input.plan.route.source.source_id.clone(),
        generation.clone(),
    );
    let result = process_generation_batches(
        &runtime,
        &input,
        &emitter,
        &generation,
        &collection,
        &diff,
        false,
        changed_total,
        &coordinator,
        &mut stage,
        &mut accumulated,
        &mut cleanup,
    )
    .await;
    if !keep_cleanup_armed {
        cleanup.disarm();
    }
    (result, stage, coordinator)
}

async fn run_actual_scheduled_generation_batches(
    adapter: Arc<ControlledBatchAdapter>,
    coordinator: Option<ProgressCoordinator>,
) -> (
    anyhow::Result<()>,
    GenerationStageProgress,
    ProgressCoordinator,
    Arc<FakeVectorStore>,
) {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ledger = Arc::new(FakeLedgerStore::new());
    let runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors.clone(),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/overlap".to_string(),
    ))
    .expect("web route")
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        false,
        None,
        None,
    );
    let execution =
        crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
    let input = SourcePipelineInput {
        adapter: adapter.as_ref(),
        plan,
        collection: "overlap-test",
        owner_id: "overlap-test",
        auth_snapshot: None,
        execution: &execution,
    };
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            &input,
            LifecycleStatus::Running,
            empty_source_counts(),
            None,
        ))
        .await
        .expect("source summary");
    let manifest = adapter.discover(&input.plan).await.expect("manifest");
    let diff = runtime
        .ledger
        .diff_manifest(manifest)
        .await
        .expect("manifest diff");
    let changed_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    let generation = diff.next_generation.clone();
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    let coordinator = coordinator.unwrap_or_else(ProgressCoordinator::test_noop);
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    let mut stage = GenerationStageProgress::default();
    let mut accumulated = GenerationAccumulator::default();
    let mut cleanup = ArtifactCleanupGuard::new(
        &runtime,
        input.plan.route.source.source_id.clone(),
        generation.clone(),
    );
    let result = Box::pin(super::scheduled::process(
        &runtime,
        &input,
        &emitter,
        &generation,
        &collection,
        &diff,
        false,
        changed_total,
        &coordinator,
        &mut stage,
        &mut accumulated,
        &mut cleanup,
    ))
    .await;
    cleanup.disarm();
    (result, stage, coordinator, vectors)
}

#[tokio::test]
async fn scheduled_generation_drops_its_sender_after_production_finishes() {
    let (acquire_started_tx, _acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        1,
        acquire_started_tx,
        normalize_started_tx,
        None,
        AdapterFailures::default(),
    ));

    let completed = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        run_actual_scheduled_generation_batches(adapter, None),
    )
    .await
    .expect("the scheduler must close its channel after the producer finishes");

    completed.0.expect("scheduled generation");
    assert_eq!(
        completed.3.calls().await,
        vec!["begin_bulk_load", "finish_bulk_load"]
    );
}

#[tokio::test]
async fn scheduled_generation_releases_prepared_work_while_next_acquisition_is_running() {
    let (acquire_started_tx, mut acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let (acquire_release_tx, acquire_release_rx) = oneshot::channel();
    let adapter = Arc::new(
        ControlledBatchAdapter::new(
            ACQUIRE_BATCH_SIZE + 1,
            acquire_started_tx,
            normalize_started_tx,
            None,
            AdapterFailures::default(),
        )
        .with_second_acquire_release(acquire_release_rx),
    );
    let coordinator = ProgressCoordinator::test_noop();
    let observed = coordinator.clone();
    let run = tokio::spawn(run_actual_scheduled_generation_batches(
        adapter,
        Some(coordinator),
    ));
    assert_eq!(acquire_started_rx.recv().await, Some(1));
    assert_eq!(acquire_started_rx.recv().await, Some(2));

    let batching_started = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if observed
                .recorded_phase_order()
                .await
                .contains(&PipelinePhase::Batching)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    acquire_release_tx
        .send(())
        .expect("release second acquisition");
    let completed = run.await.expect("scheduled batch runner");
    completed.0.expect("scheduled generation");
    assert!(
        batching_started,
        "prepared work must reach embedding while the next acquisition is still running"
    );
}

#[tokio::test]
async fn process_generation_batches_prefetches_one_batch_while_processing_the_current_batch() {
    let (acquire_started_tx, mut acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, mut normalize_started_rx) = mpsc::unbounded_channel();
    let (normalize_release_tx, normalize_release_rx) = oneshot::channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        ACQUIRE_BATCH_SIZE * 2 + 1,
        acquire_started_tx,
        normalize_started_tx,
        Some(normalize_release_rx),
        AdapterFailures::default(),
    ));

    let run = tokio::spawn(run_actual_generation_batches(adapter, None, false));
    assert_eq!(normalize_started_rx.recv().await, Some(1));
    assert_eq!(acquire_started_rx.recv().await, Some(1));
    assert_eq!(acquire_started_rx.try_recv(), Ok(2));
    assert_eq!(
        acquire_started_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty),
        "the third acquisition must not start while the first batch is still processing"
    );
    normalize_release_tx
        .send(())
        .expect("release first normalization");

    let (result, stage, coordinator) = run.await.expect("batch runner");
    result.expect("all three batches");
    assert_eq!(stage.acquired_items, (ACQUIRE_BATCH_SIZE * 2 + 1) as u64);
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Normalizing)
            .await
            .expect("normalizing progress")
            .documents_done,
        (ACQUIRE_BATCH_SIZE * 2 + 1) as u64
    );
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Fetching)
            .await
            .expect("fetching progress")
            .items_done,
        (ACQUIRE_BATCH_SIZE * 2 + 1) as u64,
        "speculative acquisitions must keep the Fetching counts advancing \
         past the first batch"
    );
    let phases = coordinator.recorded_phase_order().await;
    assert_eq!(
        phases
            .iter()
            .filter(|phase| **phase == PipelinePhase::Fetching)
            .count(),
        1,
        "speculative acquisition must not regress the published phase"
    );
}

#[tokio::test]
async fn removal_only_diff_skips_acquisition_and_completes() {
    let (acquire_started_tx, mut acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        1,
        acquire_started_tx,
        normalize_started_tx,
        None,
        AdapterFailures::default(),
    ));

    let (result, stage, _) =
        run_actual_generation_batches_with_diff(adapter, None, false, |diff| {
            let removed = std::mem::take(&mut diff.added);
            diff.counts.added = 0;
            diff.counts.removed = removed.len() as u64;
            diff.removed = removed;
        })
        .await;

    result.expect(
        "a removal-only diff has zero changed acquisition batches and must fall \
         through to finalization instead of failing the generation (H1)",
    );
    assert_eq!(stage.acquired_items, 0);
    assert!(
        acquire_started_rx.try_recv().is_err(),
        "removal-only diffs must not acquire anything"
    );
}

#[tokio::test]
async fn failed_only_diff_skips_acquisition_and_completes() {
    let (acquire_started_tx, mut acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        1,
        acquire_started_tx,
        normalize_started_tx,
        None,
        AdapterFailures::default(),
    ));

    let (result, stage, _) =
        run_actual_generation_batches_with_diff(adapter, None, false, |diff| {
            let failed = std::mem::take(&mut diff.added);
            diff.counts.added = 0;
            diff.counts.failed = failed.len() as u64;
            diff.failed = failed
                .into_iter()
                .map(|item| ManifestItemFailure {
                    item,
                    error: SourceError {
                        code: "source.item_failed".to_string(),
                        severity: Severity::Failed,
                        message: "synthetic failed manifest item".to_string(),
                        source_item_key: None,
                        retryable: true,
                        provider_id: None,
                        cause: None,
                    },
                })
                .collect();
        })
        .await;

    result.expect(
        "a failed-only diff has zero changed acquisition batches and must fall \
         through to finalization instead of failing the generation (H1)",
    );
    assert_eq!(stage.acquired_items, 0);
    assert!(
        acquire_started_rx.try_recv().is_err(),
        "failed-only diffs must not acquire anything"
    );
}

#[tokio::test]
async fn process_generation_batches_accounts_for_completed_work_before_prefetch_failure() {
    let (acquire_started_tx, _acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        ACQUIRE_BATCH_SIZE + 1,
        acquire_started_tx,
        normalize_started_tx,
        None,
        AdapterFailures {
            acquire_call: Some(2),
            normalize_call: None,
        },
    ));

    let (result, stage, coordinator) = run_actual_generation_batches(adapter, None, false).await;
    let error = result.expect_err("second acquisition fails");

    assert!(format!("{error:#}").contains("acquire 2 failed"));
    assert_eq!(stage.acquired_items, ACQUIRE_BATCH_SIZE as u64);
    assert_eq!(stage.acquired_documents, ACQUIRE_BATCH_SIZE as u64);
    assert_eq!(stage.normalized_documents, ACQUIRE_BATCH_SIZE as u64);
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Normalizing)
            .await
            .expect("completed current batch progress")
            .documents_done,
        ACQUIRE_BATCH_SIZE as u64
    );
}

#[tokio::test]
async fn process_generation_batches_preserves_processing_error_and_prefetch_context() {
    let (acquire_started_tx, _acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(ControlledBatchAdapter::new(
        ACQUIRE_BATCH_SIZE + 1,
        acquire_started_tx,
        normalize_started_tx,
        None,
        AdapterFailures {
            acquire_call: Some(2),
            normalize_call: Some(1),
        },
    ));

    let (result, stage, _) = run_actual_generation_batches(adapter, None, false).await;
    let error = result.expect_err("both overlapped operations fail");

    assert!(
        error
            .root_cause()
            .to_string()
            .contains("normalize 1 failed")
    );
    assert!(format!("{error:#}").contains("acquire 2 failed"));
    assert_eq!(stage.acquired_items, ACQUIRE_BATCH_SIZE as u64);
    assert_eq!(stage.normalized_documents, 0);
}

#[tokio::test]
async fn process_failure_cleans_artifacts_from_successful_speculative_acquisition() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let handle = core
        .put(ArtifactWriteRequest {
            kind: ArtifactKind::RawContent,
            content_type: "text/plain".to_string(),
            content: ContentRef::InlineText {
                text: "prefetched artifact".to_string(),
            },
            source_id: Some(SourceId::new("src-overlap-test")),
            job_id: Some(JobId::new(uuid::Uuid::from_u128(1))),
            metadata: MetadataMap::new(),
        })
        .await
        .expect("store prefetched artifact");
    let artifact = ArtifactRef {
        artifact_id: handle.artifact_id.clone(),
        artifact_kind: handle.artifact_kind,
        uri: handle.uri.clone().expect("artifact uri"),
        size_bytes: None,
        content_hash: None,
        created_at: Timestamp::from(chrono::Utc::now()),
    };
    let (acquire_started_tx, _acquire_started_rx) = mpsc::unbounded_channel();
    let (normalize_started_tx, _normalize_started_rx) = mpsc::unbounded_channel();
    let adapter = Arc::new(
        ControlledBatchAdapter::new(
            ACQUIRE_BATCH_SIZE + 1,
            acquire_started_tx,
            normalize_started_tx,
            None,
            AdapterFailures {
                acquire_call: None,
                normalize_call: Some(1),
            },
        )
        .with_prefetched_artifact(artifact),
    );

    let (result, _, _) =
        run_actual_generation_batches(adapter, Some(core.clone() as Arc<dyn ArtifactStore>), true)
            .await;
    assert!(result.is_err(), "current-batch processing must fail");

    let deleted = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if core.get(handle.clone()).await.is_err() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        deleted.is_ok(),
        "successful speculative acquisition artifacts must be cleaned"
    );
}

#[tokio::test]
async fn opt_in_step_overlaps_exactly_one_next_acquisition() {
    let (process_started_tx, process_started_rx) = oneshot::channel();
    let (process_release_tx, process_release_rx) = oneshot::channel();
    let (acquire_started_tx, acquire_started_rx) = oneshot::channel();
    let (acquire_release_tx, acquire_release_rx) = oneshot::channel();

    let providers = Arc::new(FakeAdapterProviders::new());
    let adapter = WebSourceAdapter::new(providers.clone(), providers);
    let step = tokio::spawn(async move {
        process_and_acquire_next(
            &adapter,
            controlled(process_started_tx, process_release_rx, Ok("processed")),
            controlled(acquire_started_tx, acquire_release_rx, Ok("acquired")),
        )
        .await
    });

    let ((), ()) = tokio::join!(
        async {
            process_started_rx.await.expect("processing started");
        },
        async {
            acquire_started_rx
                .await
                .expect("next acquisition starts before processing completes");
        }
    );
    process_release_tx.send(()).expect("release processing");
    acquire_release_tx.send(()).expect("release acquisition");

    let (processed, acquired) = step.await.expect("overlap task");
    assert_eq!(processed.expect("processed result"), "processed");
    assert_eq!(
        acquired
            .expect("one bounded lookahead")
            .expect("acquired result"),
        "acquired"
    );
}

#[tokio::test]
async fn non_opt_in_step_does_not_poll_acquisition_until_processing_finishes() {
    let (process_started_tx, process_started_rx) = oneshot::channel();
    let (process_release_tx, process_release_rx) = oneshot::channel();
    let (acquire_started_tx, mut acquire_started_rx) = oneshot::channel();
    let (acquire_release_tx, acquire_release_rx) = oneshot::channel();

    let adapter = FakeSourceAdapter::new(AdapterRef {
        name: "local".into(),
        version: "test".into(),
    });
    let step = tokio::spawn(async move {
        process_and_acquire_next(
            &adapter,
            controlled(process_started_tx, process_release_rx, Ok("processed")),
            controlled(acquire_started_tx, acquire_release_rx, Ok("acquired")),
        )
        .await
    });

    process_started_rx.await.expect("processing started");
    assert!(
        matches!(
            acquire_started_rx.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ),
        "non-opt-in acquisition must remain unpolled"
    );
    process_release_tx.send(()).expect("release processing");
    acquire_started_rx
        .await
        .expect("acquisition starts after processing completes");
    acquire_release_tx.send(()).expect("release acquisition");

    let (processed, acquired) = step.await.expect("serial task");
    assert_eq!(processed.expect("processed result"), "processed");
    assert_eq!(
        acquired
            .expect("serial next acquisition")
            .expect("acquired result"),
        "acquired"
    );
}

#[test]
fn completed_batch_is_absorbed_before_prefetch_failure_is_returned() {
    let mut absorbed = Vec::new();
    let error = resolve_batch_step::<_, ()>(
        Ok("processed"),
        Some(Err(anyhow::anyhow!("prefetch failed"))),
        |value| {
            absorbed.push(value);
            Ok(())
        },
    )
    .expect_err("prefetch failure");

    assert_eq!(absorbed, ["processed"]);
    assert_eq!(error.to_string(), "prefetch failed");
}

#[test]
fn dual_failure_keeps_processing_error_primary_and_attaches_prefetch_context() {
    let error = resolve_batch_step::<(), ()>(
        Err(anyhow::anyhow!("processing failed")),
        Some(Err(anyhow::anyhow!("prefetch failed"))),
        |_| -> anyhow::Result<()> { panic!("failed processing must not be absorbed") },
    )
    .expect_err("both operations fail");

    assert_eq!(error.root_cause().to_string(), "processing failed");
    assert!(format!("{error:#}").contains("prefetch failed"));
}

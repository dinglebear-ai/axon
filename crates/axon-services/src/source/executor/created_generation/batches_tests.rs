use super::*;
use async_trait::async_trait;
use axon_adapters::boundary::FakeAdapterProviders;
use axon_adapters::{FakeSourceAdapter, SourceAdapter, web::WebSourceAdapter};
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
    failures: AdapterFailures,
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
            failures,
        }
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
        if self.failures.acquire_call == Some(call) {
            return Err(Self::error(
                ErrorStage::Fetching,
                format!("acquire {call} failed"),
            ));
        }
        self.inner.acquire(plan, diff).await
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
) -> (
    anyhow::Result<()>,
    GenerationStageProgress,
    ProgressCoordinator,
) {
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ledger = Arc::new(FakeLedgerStore::new());
    let runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
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
    let changed_total = diff.added.len() as u64;
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
    cleanup.disarm();
    (result, stage, coordinator)
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

    let run = tokio::spawn(run_actual_generation_batches(adapter));
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

    let (result, stage, coordinator) = run_actual_generation_batches(adapter).await;
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

    let (result, stage, _) = run_actual_generation_batches(adapter).await;
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
        |value| absorbed.push(value),
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
        |_| panic!("failed processing must not be absorbed"),
    )
    .expect_err("both operations fail");

    assert_eq!(error.root_cause().to_string(), "processing failed");
    assert!(format!("{error:#}").contains("prefetch failed"));
}

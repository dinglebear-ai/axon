use super::*;
use async_trait::async_trait;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore as _;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorStore, VectorStore};
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

fn vector_write(points: u64) -> VectorStoreWriteResult {
    VectorStoreWriteResult {
        header: StageResultHeader {
            job_id: JobId::new(uuid::Uuid::from_u128(1)),
            stage_id: StageId::new(uuid::Uuid::from_u128(2)),
            phase: PipelinePhase::Upserting,
            status: LifecycleStatus::Completed,
            started_at: timestamp(),
            completed_at: Some(timestamp()),
            counts: StageCounts {
                items_total: None,
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        collection: "overlap-test".into(),
        points_attempted: points,
        points_written: points,
        payload_indexes_created: Vec::new(),
        usage: ProviderUsage {
            input_tokens: None,
            output_tokens: None,
            requests: 1,
            duration_ms: 0,
        },
    }
}

fn embedding_result(vectors: usize) -> EmbeddingResult {
    EmbeddingResult {
        batch_id: BatchId::new(uuid::Uuid::from_u128(3)),
        job_id: JobId::new(uuid::Uuid::from_u128(1)),
        provider_id: ProviderId::new("test-embedding"),
        model: "test-model".into(),
        dimensions: 1,
        vectors: (0..vectors)
            .map(|index| EmbeddingVector {
                chunk_id: ChunkId::new(format!("chunk-{index}")),
                values: vec![index as f32],
            })
            .collect(),
        usage: ProviderUsage {
            input_tokens: None,
            output_tokens: None,
            requests: 1,
            duration_ms: 0,
        },
        warnings: Vec::new(),
    }
}

async fn controlled<T>(
    started: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _ = started.send(());
    let _ = release.await;
    result
}

struct ControlledEmbeddingProvider {
    inner: FakeEmbeddingProvider,
    started: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
    fail: bool,
}

impl ControlledEmbeddingProvider {
    fn new(
        started: Option<oneshot::Sender<()>>,
        release: Option<oneshot::Receiver<()>>,
        fail: bool,
    ) -> Self {
        Self {
            inner: FakeEmbeddingProvider::new("fake-embedding", 3),
            started: Mutex::new(started),
            release: Mutex::new(release),
            fail,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for ControlledEmbeddingProvider {
    async fn embed(
        &self,
        batch: EmbeddingBatch,
    ) -> axon_embedding::provider::Result<EmbeddingResult> {
        if let Some(started) = self.started.lock().await.take() {
            let _ = started.send(());
        }
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        if self.fail {
            return Err(ApiError::new(
                "embedding.controlled.failure",
                ErrorStage::Embedding,
                "embedding failed",
            ));
        }
        Ok(EmbeddingResult {
            batch_id: batch.batch_id,
            job_id: batch.job_id,
            provider_id: batch.provider_id,
            model: batch.model,
            dimensions: 3,
            vectors: batch
                .items
                .into_iter()
                .map(|item| EmbeddingVector {
                    chunk_id: item.chunk_id,
                    values: vec![0.1, 0.2, 0.3],
                })
                .collect(),
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests: 1,
                duration_ms: 0,
            },
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> axon_embedding::provider::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}

struct ControlledVectorStore {
    inner: FakeVectorStore,
    started: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
    fail: bool,
}

impl ControlledVectorStore {
    fn new(
        started: Option<oneshot::Sender<()>>,
        release: Option<oneshot::Receiver<()>>,
        fail: bool,
    ) -> Self {
        Self {
            inner: FakeVectorStore::new("fake-vector"),
            started: Mutex::new(started),
            release: Mutex::new(release),
            fail,
        }
    }
}

#[async_trait]
impl VectorStore for ControlledVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> axon_vectors::store::Result<()> {
        self.inner.ensure_collection(spec).await
    }

    async fn upsert(
        &self,
        batch: VectorPointBatch,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        if let Some(started) = self.started.lock().await.take() {
            let _ = started.send(());
        }
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        if self.fail {
            return Err(ApiError::new(
                "vector.controlled.failure",
                ErrorStage::Upserting,
                "upsert failed",
            ));
        }
        self.inner.upsert(batch).await
    }

    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .mark_generation_committed(collection, source_id, generation)
            .await
    }

    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .mark_unchanged_items_committed(
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await
    }

    async fn retire_generation(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
        retired_epoch: SourceGenerationId,
    ) -> axon_vectors::store::Result<VectorStoreWriteResult> {
        self.inner
            .retire_generation(collection, source_id, generation, retired_epoch)
            .await
    }

    async fn delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> axon_vectors::store::Result<VectorStoreDeleteResult> {
        self.inner.delete(selector).await
    }

    async fn search(
        &self,
        request: VectorSearchRequest,
    ) -> axon_vectors::store::Result<VectorSearchResult> {
        self.inner.search(request).await
    }

    async fn capabilities(&self) -> axon_vectors::store::Result<ProviderCapability> {
        self.inner.capabilities().await
    }
}

/// Records the phases of durable job-store heartbeats emitted by provider
/// calls, delegating everything else to a [`FakeJobWatchStore`].
struct HeartbeatRecordingJobStore {
    inner: axon_jobs::boundary::FakeJobWatchStore,
    phases: std::sync::Mutex<Vec<PipelinePhase>>,
}

impl HeartbeatRecordingJobStore {
    fn new() -> Self {
        Self {
            inner: axon_jobs::boundary::FakeJobWatchStore::new(),
            phases: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn recorded_phases(&self) -> Vec<PipelinePhase> {
        self.phases.lock().expect("heartbeat phase mutex").clone()
    }
}

#[async_trait]
impl axon_jobs::boundary::JobStore for HeartbeatRecordingJobStore {
    async fn create(
        &self,
        request: JobCreateRequest,
    ) -> axon_jobs::boundary::Result<JobDescriptor> {
        self.inner.create(request).await
    }

    async fn admit_projection_batch_atomic(
        &self,
        admission: ProjectionBatchAdmission,
    ) -> axon_jobs::boundary::Result<ProjectionBatchAdmissionResult> {
        self.inner.admit_projection_batch_atomic(admission).await
    }

    async fn projection_batch(
        &self,
        lookup: ProjectionBatchLookup,
    ) -> axon_jobs::boundary::Result<Option<ProjectionBatchAdmissionResult>> {
        self.inner.projection_batch(lookup).await
    }

    async fn get(&self, job_id: JobId) -> axon_jobs::boundary::Result<Option<JobSummary>> {
        self.inner.get(job_id).await
    }

    async fn attempts(
        &self,
        job_id: JobId,
    ) -> axon_jobs::boundary::Result<Vec<JobAttemptSnapshot>> {
        self.inner.attempts(job_id).await
    }

    async fn stages(&self, job_id: JobId) -> axon_jobs::boundary::Result<Vec<JobStageSnapshot>> {
        self.inner.stages(job_id).await
    }

    async fn update_status(&self, status: JobStatusUpdate) -> axon_jobs::boundary::Result<()> {
        self.inner.update_status(status).await
    }

    async fn append_event(&self, event: SourceProgressEvent) -> axon_jobs::boundary::Result<()> {
        self.inner.append_event(event).await
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> axon_jobs::boundary::Result<()> {
        self.phases
            .lock()
            .expect("heartbeat phase mutex")
            .push(heartbeat.phase);
        Ok(())
    }

    async fn list(&self, request: JobListRequest) -> axon_jobs::boundary::Result<Page<JobSummary>> {
        self.inner.list(request).await
    }

    async fn events(
        &self,
        request: JobEventListRequest,
    ) -> axon_jobs::boundary::Result<JobEventPage> {
        self.inner.events(request).await
    }

    async fn latest_event_sequence(
        &self,
        job_id: JobId,
    ) -> axon_jobs::boundary::Result<Option<u64>> {
        self.inner.latest_event_sequence(job_id).await
    }

    async fn cancel(
        &self,
        job_id: JobId,
        request: JobCancelRequest,
    ) -> axon_jobs::boundary::Result<JobCancelResult> {
        self.inner.cancel(job_id, request).await
    }

    async fn retry(
        &self,
        job_id: JobId,
        request: JobRetryRequest,
    ) -> axon_jobs::boundary::Result<JobRetryResult> {
        self.inner.retry(job_id, request).await
    }

    async fn recover(
        &self,
        request: JobRecoveryRequest,
    ) -> axon_jobs::boundary::Result<JobRecoveryResult> {
        self.inner.recover(request).await
    }

    async fn cleanup(
        &self,
        request: JobCleanupRequest,
    ) -> axon_jobs::boundary::Result<JobCleanupResult> {
        self.inner.cleanup(request).await
    }

    async fn delete_jobs(
        &self,
        job_ids: &[JobId],
    ) -> axon_jobs::boundary::Result<axon_jobs::boundary::JobDeleteResult> {
        self.inner.delete_jobs(job_ids).await
    }

    async fn artifacts(
        &self,
        request: JobArtifactListRequest,
    ) -> axon_jobs::boundary::Result<JobArtifactListResult> {
        self.inner.artifacts(request).await
    }

    async fn reset(&self) -> axon_jobs::boundary::Result<()> {
        self.inner.reset().await
    }

    async fn capabilities(&self) -> axon_jobs::boundary::Result<JobStoreCapability> {
        self.inner.capabilities().await
    }
}

async fn run_actual_publish_and_build_next(
    embedding_provider: Arc<ControlledEmbeddingProvider>,
    vector_store: Arc<ControlledVectorStore>,
) -> (
    anyhow::Result<BuiltVectorBatch>,
    VectorizeResult,
    ProgressCoordinator,
) {
    run_actual_publish_and_build_next_with_jobs(
        embedding_provider,
        vector_store,
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
    )
    .await
}

async fn run_actual_publish_and_build_next_with_jobs(
    embedding_provider: Arc<ControlledEmbeddingProvider>,
    vector_store: Arc<ControlledVectorStore>,
    jobs: Arc<dyn axon_jobs::boundary::JobStore>,
) -> (
    anyhow::Result<BuiltVectorBatch>,
    VectorizeResult,
    ProgressCoordinator,
) {
    let collection = axon_vectors::testing::test_collection_spec_hybrid(3);
    vector_store
        .ensure_collection(collection.clone())
        .await
        .expect("test collection");
    let runtime = TargetLocalSourceRuntime::new(
        jobs,
        Arc::new(FakeLedgerStore::new()),
        embedding_provider,
        vector_store,
        ProviderId::new("fake-embedding"),
        "text-embedding-test",
        3,
    );
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/vector-overlap".to_string(),
    ))
    .expect("web route")
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        true,
        None,
        None,
    );
    let execution =
        crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter = axon_adapters::FakeSourceAdapter::new(route.adapter.clone());
    let collection_name = collection.collection.clone();
    let input = SourcePipelineInput {
        adapter: &adapter,
        plan,
        collection: &collection_name,
        owner_id: "vector-overlap-test",
        auth_snapshot: None,
        execution: &execution,
    };
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();
    let current_document = axon_vectors::testing::test_prepared_document();
    let mut current_embeddings = axon_vectors::testing::test_embedding_result_for(
        &current_document,
        "text-embedding-test",
        3,
    );
    let VectorPointBuild {
        batch: point_batch,
        skipped_redaction,
        redaction_skips_by_source_item,
        points_by_document,
    } = point_batch(
        collection.clone(),
        std::slice::from_ref(&current_document),
        &mut current_embeddings,
    )
    .expect("current point batch");
    let current = BuiltVectorBatch {
        documents: vec![current_document],
        embedding_warnings: Vec::new(),
        point_batch,
        points_by_document,
        skipped_redaction,
        redaction_skips_by_source_item,
    };
    let mut next_document = axon_vectors::testing::test_prepared_document();
    next_document.metadata.remove("embedding_batch_id");
    let next_documents = vec![next_document];
    let mut output = VectorizeResult::default();
    let result = publish_and_build_next(
        &runtime,
        &input,
        current,
        next_documents,
        collection.clone(),
        &emitter,
        &coordinator,
        &mut progress,
        &mut output,
        true,
    )
    .await;
    (result, output, coordinator)
}

#[tokio::test]
async fn publish_and_build_next_overlaps_real_provider_calls_and_checkpoints_in_output_order() {
    let (upsert_started_tx, upsert_started_rx) = oneshot::channel();
    let (upsert_release_tx, upsert_release_rx) = oneshot::channel();
    let (embed_started_tx, mut embed_started_rx) = oneshot::channel();
    let (embed_release_tx, embed_release_rx) = oneshot::channel();
    let embedding = Arc::new(ControlledEmbeddingProvider::new(
        Some(embed_started_tx),
        Some(embed_release_rx),
        false,
    ));
    let vectors = Arc::new(ControlledVectorStore::new(
        Some(upsert_started_tx),
        Some(upsert_release_rx),
        false,
    ));

    let run = tokio::spawn(run_actual_publish_and_build_next(embedding, vectors));
    upsert_started_rx.await.expect("current upsert started");
    assert_eq!(
        embed_started_rx.try_recv(),
        Ok(()),
        "next embedding must start before current upsert completes"
    );
    embed_release_tx.send(()).expect("release embedding first");
    upsert_release_tx.send(()).expect("release upsert second");

    let (result, output, coordinator) = run.await.expect("publish runner");
    let next = result.expect("overlapped publish and build");
    assert_eq!(output.points_written, 2);
    assert_eq!(next.point_batch.points.len(), 2);
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Upserting)
            .await
            .expect("upsert checkpoint")
            .chunks_done,
        2
    );
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Embedding)
            .await
            .expect("embedding checkpoint")
            .chunks_done,
        2
    );
    assert_eq!(
        coordinator.recorded_phase_order().await,
        vec![
            PipelinePhase::Upserting,
            PipelinePhase::Embedding,
            PipelinePhase::Vectorizing,
        ]
    );
}

#[tokio::test]
async fn publish_and_build_next_preserves_upsert_error_and_embedding_context() {
    let embedding = Arc::new(ControlledEmbeddingProvider::new(None, None, true));
    let vectors = Arc::new(ControlledVectorStore::new(None, None, true));

    let (result, output, coordinator) = run_actual_publish_and_build_next(embedding, vectors).await;
    let error = match result {
        Ok(_) => panic!("both real call-site operations must fail"),
        Err(error) => error,
    };

    assert!(error.root_cause().to_string().contains("upsert failed"));
    assert!(format!("{error:#}").contains("embedding failed"));
    assert_eq!(
        output.points_written, 0,
        "a failed upsert must not be absorbed as completed work"
    );
    assert_eq!(
        coordinator.recorded_phase_order().await,
        vec![PipelinePhase::Upserting],
        "speculative embedding must not replace the active upsert phase"
    );
    assert_eq!(
        coordinator
            .latest_counts(PipelinePhase::Upserting)
            .await
            .expect("upsert attempt progress")
            .chunks_done,
        0
    );
}

#[tokio::test]
async fn overlapped_step_records_monotonic_job_store_heartbeat_phases() {
    let jobs = Arc::new(HeartbeatRecordingJobStore::new());
    let embedding = Arc::new(ControlledEmbeddingProvider::new(None, None, false));
    let vectors = Arc::new(ControlledVectorStore::new(None, None, false));

    let (result, output, _) =
        run_actual_publish_and_build_next_with_jobs(embedding, vectors, jobs.clone()).await;
    result.expect("overlapped publish and build");
    assert_eq!(output.points_written, 2);

    let phases = jobs.recorded_phases();
    assert!(
        !phases.is_empty(),
        "provider calls must record durable heartbeats"
    );
    assert!(
        phases
            .iter()
            .all(|phase| *phase == PipelinePhase::Upserting),
        "the speculative embedding call must heartbeat the still-published \
         Upserting phase instead of leaking Embedding ahead of the \
         ProgressCoordinator (finding M2); recorded phases: {phases:?}"
    );
}

#[tokio::test]
async fn overlapped_embedding_failure_absorbs_the_successful_upsert_accounting() {
    let embedding = Arc::new(ControlledEmbeddingProvider::new(None, None, true));
    let vectors = Arc::new(ControlledVectorStore::new(None, None, false));

    let (result, output, coordinator) = run_actual_publish_and_build_next(embedding, vectors).await;
    let error = match result {
        Ok(_) => panic!("speculative embedding failure must fail the step"),
        Err(error) => error,
    };

    let rendered = format!("{error:#}");
    assert!(rendered.contains("embedding failed"), "{rendered}");
    assert_eq!(
        output.points_written, 2,
        "the checkpointed current write's accounting must reach the failure \
         summary before the overlapped embedding error propagates"
    );
    assert_eq!(
        output.documents_prepared, 1,
        "the current batch's document statuses must be absorbed"
    );
    assert_eq!(
        coordinator.recorded_phase_order().await,
        vec![PipelinePhase::Upserting],
        "the successful upsert is checkpointed before the embedding failure"
    );
}

#[tokio::test]
async fn prepared_pool_checkpoints_successful_upsert_before_next_embedding_failure() {
    let embedding = Arc::new(ControlledEmbeddingProvider::new(None, None, true));
    let vectors = Arc::new(ControlledVectorStore::new(None, None, false));
    let ledger = Arc::new(FakeLedgerStore::new());
    let collection = axon_vectors::testing::test_collection_spec_hybrid(3);
    vectors
        .ensure_collection(collection.clone())
        .await
        .expect("test collection");
    let runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger.clone(),
        embedding,
        vectors,
        ProviderId::new("fake-embedding"),
        "text-embedding-test",
        3,
    );
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/vector-overlap-checkpoint".to_string(),
    ))
    .expect("web route")
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        true,
        None,
        None,
    );
    let execution =
        crate::source::execution::SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter = axon_adapters::FakeSourceAdapter::new(route.adapter.clone());
    let input = SourcePipelineInput {
        adapter: &adapter,
        plan,
        collection: &collection.collection,
        owner_id: "vector-overlap-checkpoint-test",
        auth_snapshot: None,
        execution: &execution,
    };
    let mut source = crate::source::executor::metadata::source_summary(
        &input,
        LifecycleStatus::Running,
        crate::source::executor::helpers::empty_source_counts(),
        None,
    );
    source.source_id = SourceId::new("src-web");
    ledger.upsert_source(source).await.expect("source summary");
    let emitter = SourceEventEmitter::new(None, Some(input.plan.job_id));
    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();

    let current_document = axon_vectors::testing::test_prepared_document();
    let current_document_id = current_document.document_id.clone();
    let mut current_embeddings = axon_vectors::testing::test_embedding_result_for(
        &current_document,
        "text-embedding-test",
        3,
    );
    let VectorPointBuild {
        batch: point_batch,
        skipped_redaction,
        redaction_skips_by_source_item,
        points_by_document,
    } = point_batch(
        collection.clone(),
        std::slice::from_ref(&current_document),
        &mut current_embeddings,
    )
    .expect("current point batch");
    let current = BuiltVectorBatch {
        documents: vec![current_document],
        embedding_warnings: Vec::new(),
        point_batch,
        points_by_document,
        skipped_redaction,
        redaction_skips_by_source_item,
    };
    let mut next_document = axon_vectors::testing::test_prepared_document();
    next_document.metadata.remove("embedding_batch_id");
    let mut vectorizer = super::super::PreparedPoolVectorizer {
        ready: Some(current),
        cumulative: std::collections::HashMap::new(),
    };

    let error = vectorizer
        .push(
            &runtime,
            &input,
            collection.clone(),
            &emitter,
            &coordinator,
            vec![next_document],
            &mut progress,
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect_err("speculative next embedding must fail");

    let rendered = format!("{error:#}");
    assert!(rendered.contains("embedding failed"), "{rendered}");
    let status = ledger
        .document_status(&current_document_id)
        .await
        .expect("successful current upsert must be checkpointed");
    assert_eq!(status.status, DocumentLifecycleStatus::Vectorized);
    assert_eq!(status.vector_point_count, 2);
}

#[tokio::test]
async fn next_embedding_overlaps_current_upsert_and_results_keep_operation_order() {
    let (upsert_started_tx, upsert_started_rx) = oneshot::channel();
    let (upsert_release_tx, upsert_release_rx) = oneshot::channel();
    let (embed_started_tx, embed_started_rx) = oneshot::channel();
    let (embed_release_tx, embed_release_rx) = oneshot::channel();

    let joined = tokio::spawn(join_upsert_and_embedding(
        controlled(upsert_started_tx, upsert_release_rx, Ok("current-write")),
        controlled(embed_started_tx, embed_release_rx, Ok("next-embeddings")),
    ));

    upsert_started_rx.await.expect("upsert started");
    embed_started_rx
        .await
        .expect("next embedding starts before upsert completes");
    embed_release_tx.send(()).expect("release embedding first");
    upsert_release_tx.send(()).expect("release upsert second");

    let (write, embeddings) = joined.await.expect("join task");
    assert_eq!(write.expect("current write"), "current-write");
    assert_eq!(embeddings.expect("next embeddings"), "next-embeddings");
}

fn points_only_result(write: VectorStoreWriteResult) -> VectorizeResult {
    let mut result = VectorizeResult::default();
    result.points_written = write.points_written;
    result
}

#[tokio::test]
async fn individual_overlap_failures_preserve_the_failing_operation() {
    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();
    let mut output = VectorizeResult::default();
    let upsert = resolve_and_checkpoint_overlap(
        &coordinator,
        &mut progress,
        &mut output,
        Err(anyhow::anyhow!("upsert failed")),
        Ok(embedding_result(1)),
        |_| panic!("a failed upsert must not be absorbed"),
    )
    .await
    .expect_err("upsert failure");
    assert_eq!(upsert.to_string(), "upsert failed");
    assert_eq!(output.points_written, 0);

    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();
    let mut output = VectorizeResult::default();
    let embedding = resolve_and_checkpoint_overlap(
        &coordinator,
        &mut progress,
        &mut output,
        Ok(vector_write(2)),
        Err(anyhow::anyhow!("embedding failed")),
        points_only_result,
    )
    .await
    .expect_err("embedding failure");
    assert_eq!(embedding.to_string(), "embedding failed");
    assert_eq!(
        output.points_written, 2,
        "the checkpointed current write is absorbed before the next embedding \
         failure surfaces"
    );
    assert_eq!(
        coordinator.recorded_phase_order().await,
        vec![PipelinePhase::Upserting],
        "successful current upsert is checkpointed before next embedding failure surfaces"
    );
}

#[tokio::test]
async fn dual_failure_keeps_upsert_primary_and_attaches_embedding_context() {
    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();
    let mut output = VectorizeResult::default();
    let error = resolve_and_checkpoint_overlap(
        &coordinator,
        &mut progress,
        &mut output,
        Err(anyhow::anyhow!("upsert failed")),
        Err(anyhow::anyhow!("embedding failed")),
        |_| panic!("a failed upsert must not be absorbed"),
    )
    .await
    .expect_err("both operations fail");

    assert_eq!(error.root_cause().to_string(), "upsert failed");
    assert!(format!("{error:#}").contains("embedding failed"));
    assert!(coordinator.recorded_phase_order().await.is_empty());
    assert_eq!(output.points_written, 0);
}

#[tokio::test]
async fn successful_overlap_checkpoints_current_upsert_before_next_embedding() {
    let coordinator = ProgressCoordinator::test_noop();
    let mut progress = PipelineProgress::default();
    let mut output = VectorizeResult::default();
    let embeddings = resolve_and_checkpoint_overlap(
        &coordinator,
        &mut progress,
        &mut output,
        Ok(vector_write(2)),
        Ok(embedding_result(3)),
        points_only_result,
    )
    .await
    .expect("overlap results");

    assert_eq!(output.points_written, 2);
    assert_eq!(embeddings.vectors.len(), 3);
    assert_eq!(
        coordinator.recorded_phase_order().await,
        vec![PipelinePhase::Upserting, PipelinePhase::Embedding]
    );
}

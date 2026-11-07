use super::*;
use axon_adapters::{FakeSourceAdapter, SourceAdapter};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore, VectorStore};
use std::sync::Arc;

struct Fixture {
    runtime: TargetLocalSourceRuntime,
    ledger: Arc<FakeLedgerStore>,
    vectors: Arc<FakeVectorStore>,
    adapter: FakeSourceAdapter,
    plan: SourcePlan,
    execution: SourceExecutionContext,
}
impl Fixture {
    async fn new(ledger: FakeLedgerStore, mode: FakeVectorMode) -> Self {
        let request = SourceRequest::local_path("/tmp/finalization", true);
        let route = crate::source::routing::resolve_source_route(&request)
            .unwrap()
            .route;
        let plan = crate::source::dispatch::family_source_plan(
            &route.source.canonical_uri,
            &route,
            true,
            None,
            None,
        );
        let execution = SourceExecutionContext::inline(plan.request.clone(), None);
        let adapter = FakeSourceAdapter::new(route.adapter).with_item(
            "readme.md",
            ContentKind::Markdown,
            "body",
        );
        let ledger = Arc::new(ledger);
        let vectors = Arc::new(FakeVectorStore::new("vectors").with_mode(mode));
        vectors
            .ensure_collection(collection_spec("test", 8))
            .await
            .unwrap();
        let runtime = TargetLocalSourceRuntime::new(
            Arc::new(FakeJobWatchStore::new()),
            ledger.clone(),
            Arc::new(FakeEmbeddingProvider::new("embedding", 8)),
            vectors.clone(),
            ProviderId::new("embedding"),
            "embedding",
            8,
        );
        let fixture = Self {
            runtime,
            ledger,
            vectors,
            adapter,
            plan,
            execution,
        };
        fixture
            .ledger
            .upsert_source(metadata::source_summary(
                &fixture.input(),
                LifecycleStatus::Running,
                empty_source_counts(),
                None,
            ))
            .await
            .unwrap();
        fixture
    }
    fn input(&self) -> SourcePipelineInput<'_> {
        SourcePipelineInput {
            graph_stage: None,
            adapter: &self.adapter,
            plan: self.plan.clone(),
            collection: "test",
            owner_id: "test",
            auth_snapshot: None,
            execution: &self.execution,
        }
    }
    async fn generation(&self) -> SourceGeneration {
        self.ledger
            .create_generation(self.plan.route.source.source_id.clone())
            .await
            .unwrap()
    }
    async fn status(&self, generation: &SourceGeneration) -> LifecycleStatus {
        self.ledger
            .generation(&generation.source_id, &generation.generation)
            .await
            .unwrap()
            .status
    }
    async fn finish(&self, generation: SourceGeneration) -> anyhow::Error {
        finalize_failed_generation(
            &self.runtime,
            &self.input(),
            generation,
            anyhow::Error::new(ApiError::new(
                "adapter.primary_failed",
                ErrorStage::Fetching,
                "primary failure",
            )),
        )
        .await
    }
    async fn publish(&self, mut generation: SourceGeneration) -> SourceGeneration {
        let mut manifest = self.adapter.discover(&self.plan).await.unwrap();
        manifest.generation = generation.generation.clone();
        self.ledger.put_manifest(manifest).await.unwrap();
        generation.status = LifecycleStatus::Completed;
        let generation = self.ledger.complete_generation(generation).await.unwrap();
        self.ledger
            .publish_generation(PublishGenerationRequest {
                job_id: self.plan.job_id,
                attempt: 1,
                source_id: generation.source_id.clone(),
                generation: generation.generation.clone(),
                expected_previous_generation: generation.previous_generation.clone(),
                retained_statuses: Vec::new(),
            })
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn manifest_write_error_inside_created_boundary_marks_generation_failed() {
    let fixture = Fixture::new(
        FakeLedgerStore::new().with_manifest_write_failure(),
        FakeVectorMode::Success,
    )
    .await;
    let error = crate::source::dispatch::dispatch_materialized(
        &fixture.runtime,
        &fixture.adapter,
        fixture.plan.clone(),
        "test",
        "test",
        None,
        &fixture.execution,
        |plan| async { Ok(axon_adapters::acquisition::MaterializedSource::virtual_source(plan)) },
    )
    .await
    .unwrap_err();
    assert!(format!("{error:#}").contains("injected manifest_write failure"));
    let attempted = fixture
        .ledger
        .generation(
            &fixture.plan.route.source.source_id,
            &SourceGenerationId::new("gen_1"),
        )
        .await
        .unwrap();
    assert_eq!(attempted.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn committed_lookup_failure_skips_delete_but_still_marks_failed() {
    let fixture = Fixture::new(
        FakeLedgerStore::new().with_committed_generation_failure(),
        FakeVectorMode::Success,
    )
    .await;
    let generation = fixture.generation().await;
    let error = fixture.finish(generation.clone()).await;
    let chain = format!("{error:#}");
    assert!(
        chain.contains("primary failure")
            && chain.contains("injected committed generation failure")
    );
    assert_eq!(fixture.status(&generation).await, LifecycleStatus::Failed);
    assert!(!fixture.vectors.calls().await.contains(&"delete"));
    assert_eq!(
        error.downcast_ref::<ApiError>().unwrap().code.to_string(),
        "adapter.primary_failed"
    );
}

#[tokio::test]
async fn vector_delete_failure_marks_failed_with_durable_debt() {
    let fixture = Fixture::new(FakeLedgerStore::new(), FakeVectorMode::DeleteFailure).await;
    let generation = fixture.generation().await;
    let error = fixture.finish(generation.clone()).await;
    assert!(format!("{error:#}").contains("deferred to durable cleanup debt"));
    assert_eq!(fixture.ledger.cleanup_debt_count().await, 1);
    assert_eq!(fixture.status(&generation).await, LifecycleStatus::Failed);
}

#[tokio::test]
async fn debt_write_failure_does_not_prevent_generation_failure_marking() {
    let fixture = Fixture::new(
        FakeLedgerStore::new().with_cleanup_debt_write_failure(),
        FakeVectorMode::DeleteFailure,
    )
    .await;
    let generation = fixture.generation().await;
    let error = format!("{:#}", fixture.finish(generation.clone()).await);
    assert!(
        error.contains("primary failure")
            && error.contains("failed to record failed-generation vector cleanup debt")
    );
    assert_eq!(fixture.status(&generation).await, LifecycleStatus::Failed);
}

#[tokio::test]
async fn independent_failure_marking_error_keeps_primary_and_cleanup_evidence() {
    let fixture = Fixture::new(
        FakeLedgerStore::new().with_fail_generation_failure(),
        FakeVectorMode::DeleteFailure,
    )
    .await;
    let generation = fixture.generation().await;
    let error = format!("{:#}", fixture.finish(generation).await);
    assert!(
        error.contains("primary failure")
            && error.contains("deferred to durable cleanup debt")
            && error.contains("injected fail_generation failure")
    );
    assert_eq!(fixture.ledger.cleanup_debt_count().await, 1);
}

#[tokio::test]
async fn published_generation_is_not_deleted_or_marked_failed_after_late_error() {
    let fixture = Fixture::new(FakeLedgerStore::new(), FakeVectorMode::Success).await;
    let generation = fixture.publish(fixture.generation().await).await;
    let error = fixture.finish(generation.clone()).await;
    assert_eq!(
        error.downcast_ref::<ApiError>().unwrap().message,
        "primary failure"
    );
    assert_eq!(error.chain().count(), 1);
    assert_eq!(
        fixture.status(&generation).await,
        LifecycleStatus::Completed
    );
    assert_eq!(
        fixture
            .ledger
            .committed_generation(&generation.source_id)
            .await,
        Some(generation.generation)
    );
    assert!(!fixture.vectors.calls().await.contains(&"delete"));
}

#[tokio::test]
async fn unknown_committed_state_cannot_fail_published_generation() {
    let fixture = Fixture::new(
        FakeLedgerStore::new().with_committed_generation_failure(),
        FakeVectorMode::Success,
    )
    .await;
    let generation = fixture.publish(fixture.generation().await).await;
    let error = format!("{:#}", fixture.finish(generation.clone()).await);
    assert!(
        error.contains("primary failure")
            && error.contains("also failed to mark source generation failed")
    );
    assert_eq!(
        fixture.status(&generation).await,
        LifecycleStatus::Completed
    );
    assert!(!fixture.vectors.calls().await.contains(&"delete"));
}

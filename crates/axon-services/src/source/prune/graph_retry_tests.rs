use super::*;
use std::sync::atomic::AtomicUsize;

struct BusyGraph {
    calls: AtomicUsize,
    busy: AtomicBool,
    retry_only: AtomicBool,
    vector_calls: AtomicUsize,
    vector_fails: AtomicBool,
    retry_generation: Option<SourceGenerationId>,
}

#[async_trait]
impl CleanupProviderOps for BusyGraph {
    fn graph_retry_generation(&self) -> Option<&SourceGenerationId> {
        self.retry_generation.as_ref()
    }

    fn graph_retry_only(&self) -> bool {
        self.retry_only.load(Ordering::SeqCst)
    }

    async fn graph_retire_item(
        &self,
        _: SourceId,
        _: SourceItemKey,
        _: SourceGenerationId,
    ) -> Result<GraphDeleteResult, ApiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.busy.load(Ordering::SeqCst) {
            return Ok(GraphDeleteResult::default());
        }
        Err(ApiError::new(
            "graph.source_busy",
            ErrorStage::Graphing,
            "source publication is busy",
        ))
    }
    async fn vector_delete(
        &self,
        _: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult, ApiError> {
        self.vector_calls.fetch_add(1, Ordering::SeqCst);
        if self.vector_fails.load(Ordering::SeqCst) {
            return Err(ApiError::new(
                "vector.storage_error",
                ErrorStage::Cleaning,
                "storage unavailable",
            ));
        }
        Ok(VectorStoreDeleteResult {
            collection: COLLECTION.into(),
            points_matched: 1,
            points_deleted: 1,
            dry_run: false,
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }
    async fn graph_delete_nodes(&self, _: Vec<String>) -> Result<GraphDeleteResult, ApiError> {
        unreachable!()
    }
    async fn graph_delete_edges(
        &self,
        _: Vec<axon_api::source::GraphEdgeId>,
    ) -> Result<GraphDeleteResult, ApiError> {
        unreachable!()
    }
    async fn artifact_delete(&self, _: ArtifactHandle) -> Result<(), ApiError> {
        unreachable!()
    }
}

#[tokio::test]
async fn busy_graph_stops_source_drain_and_persists_retry_without_resolving_debt() {
    let (ledger, generation) = graph_retry_fixture().await;
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: None,
    };
    let summary = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(
        providers.calls.load(Ordering::SeqCst),
        1,
        "a busy source must not retry every item in the same sweep"
    );
    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.failed, 1);
    let pending = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    assert_eq!(pending.len(), 3);
    let retried = pending.iter().find(|debt| debt.attempts == 1).unwrap();
    assert!(retried.next_retry_at.as_ref().unwrap().0 > Timestamp::from(chrono::Utc::now()).0);
    assert_eq!(
        retried.last_error.as_ref().unwrap().code,
        "graph.source_busy"
    );
    let deferred = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(
        deferred.failed, 3,
        "pending backoff debt must remain visible as degraded cleanup"
    );
    assert_eq!(
        providers.calls.load(Ordering::SeqCst),
        1,
        "busy backoff applies to all graph debts for the source"
    );
    let mut vector_debt = cleanup_debt(
        CleanupDebtKind::VectorDelete,
        CleanupSelector::SourceItem {
            source_id: SourceId::new(SRC),
            source_item_key: SourceItemKey::new("old"),
            generation: SourceGenerationId::new("old-generation"),
        },
    );
    vector_debt.generation = Some(SourceGenerationId::new("old-generation"));
    vector_debt.vector_collection = Some(COLLECTION.into());
    ledger.record_cleanup_debt(vector_debt).await.unwrap();
    let mixed = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(mixed.resolved, 1);
    assert_eq!(
        mixed.failed, 3,
        "successful vector cleanup must not hide pending graph debt"
    );
    assert_pending_audit(&deferred).await;
    assert_pending_audit(&mixed).await;
    let target = LedgerPruneTarget {
        provider_ops: &providers,
        ledger: &ledger,
        memory_store: None,
        job_store: None,
        source_id: SourceId::new(SRC),
        committed_generation: generation.generation.clone(),
        job_ids: Vec::new(),
    };
    let executor = PruneExecutor::new(target);
    let mut retry_summary = DebtDrainSummary::default();
    drain_one_debt(
        &ledger,
        &executor,
        &PruneAuthz::admin(),
        retried,
        COLLECTION,
        &providers,
        None,
        None,
        &mut retry_summary,
    )
    .await;
    assert_eq!(
        providers.calls.load(Ordering::SeqCst),
        1,
        "future retry must not call graph provider"
    );
    assert_eq!(retry_summary.failed, 1);
    // A due retry resumes cleanup; no debt is abandoned after contention.
    let mut due = retried.clone();
    due.next_retry_at = Some(Timestamp::from(
        chrono::Utc::now() - chrono::Duration::seconds(1),
    ));
    ledger.record_cleanup_debt(due).await.unwrap();
    providers.busy.store(false, Ordering::SeqCst);
    let resumed = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(resumed.resolved, 3);
    assert_eq!(resumed.failed, 0);
    assert_eq!(providers.calls.load(Ordering::SeqCst), 4);
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .is_empty()
    );
}

async fn graph_retry_fixture() -> (FakeLedgerStore, SourceGeneration) {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    let generation = ledger.create_generation(SourceId::new(SRC)).await.unwrap();
    ledger
        .put_manifest(manifest(
            &generation.generation.0,
            vec![("current", "same")],
        ))
        .await
        .unwrap();
    let generation = publish(&ledger, completed(generation)).await;
    for item in ["removed-one", "removed-two", "removed-three"] {
        let mut debt = cleanup_debt(
            CleanupDebtKind::GraphPrune,
            CleanupSelector::GraphItemEvidence {
                source_id: SourceId::new(SRC),
                source_item_key: SourceItemKey::new(item),
                retirement_generation: generation.generation.clone(),
            },
        );
        debt.debt_id = CleanupDebtId::new(item);
        ledger.record_cleanup_debt(debt).await.unwrap();
    }
    (ledger, generation)
}

async fn run_retry(
    ledger: &FakeLedgerStore,
    providers: &BusyGraph,
    generation: &SourceGenerationId,
) -> DebtDrainSummary {
    drain_cleanup_debt_with_provider_ops(
        ledger,
        providers,
        None,
        None,
        None,
        None,
        COLLECTION,
        &index_counts(generation),
    )
    .await
}

async fn assert_pending_audit(summary: &DebtDrainSummary) {
    let warning = crate::source::job_tracking::prune_outcome_warning(summary).unwrap();
    assert_eq!(warning.code, "source.prune.cleanup_deferred");
    let jobs = std::sync::Arc::new(FakeJobWatchStore::new());
    let job_id = create_terminal_job(jobs.as_ref()).await;
    assert!(
        crate::source::job_tracking::track_prune(Some(jobs.clone()), job_id, None, summary)
            .await
            .is_none()
    );
    let events = jobs
        .events(axon_api::source::JobEventListRequest {
            job_id,
            after_sequence: None,
            limit: Some(20),
            severity: None,
            visibility: None,
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await
        .unwrap()
        .events;
    let event = events.last().unwrap();
    assert_eq!(event.phase, PipelinePhase::Cleaning);
    assert_eq!(event.status, LifecycleStatus::Failed);
    assert_eq!(event.severity, axon_api::source::Severity::Warning);
    assert!(event.message.contains("failed=3"));
}

#[tokio::test]
async fn post_publication_finalizer_retries_current_busy_debt_without_waiting() {
    let (ledger, generation) = graph_retry_fixture().await;
    let mut providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: None,
    };
    let busy = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(busy.failed, 1);
    providers.busy.store(false, Ordering::SeqCst);
    // Autonomous sweeps still honor the persisted delay despite availability.
    let autonomous = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(autonomous.failed, 3);
    assert_eq!(providers.calls.load(Ordering::SeqCst), 1);
    // A different generation cannot override this source's delayed retirement.
    providers.retry_generation = Some(SourceGenerationId::new("different-generation"));
    let wrong_generation = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(wrong_generation.failed, 3);
    assert_eq!(providers.calls.load(Ordering::SeqCst), 1);
    providers.retry_generation = Some(generation.generation.clone());
    let finalized = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(
        finalized.failed, 0,
        "available finalizer must not degrade solely due to stale busy delay"
    );
    assert_eq!(finalized.resolved, 3);
    assert_eq!(providers.calls.load(Ordering::SeqCst), 4);
    assert!(crate::source::job_tracking::prune_outcome_warning(&finalized).is_none());
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn finalizer_preserves_nonbusy_delays_and_actual_busy_failures() {
    let (ledger, generation) = graph_retry_fixture().await;
    let pending = ledger
        .list_pending_cleanup_debt(SourceId::new(SRC))
        .await
        .unwrap();
    let mut delayed = pending[0].clone();
    delayed.next_retry_at = Some(Timestamp::from(
        chrono::Utc::now() + chrono::Duration::seconds(300),
    ));
    delayed.last_error = Some(axon_api::source::SourceError {
        code: "graph.storage_error".into(),
        severity: axon_api::source::Severity::Warning,
        message: "storage unavailable".into(),
        source_item_key: None,
        retryable: true,
        provider_id: None,
        cause: None,
    });
    ledger.record_cleanup_debt(delayed).await.unwrap();
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        retry_generation: Some(generation.generation.clone()),
    };
    let nonbusy = run_retry(&ledger, &providers, &generation.generation).await;
    assert_eq!(nonbusy.resolved, 2);
    assert_eq!(nonbusy.failed, 1);
    assert_eq!(
        providers.calls.load(Ordering::SeqCst),
        2,
        "storage-error delay is preserved by finalizer"
    );
    let (busy_ledger, busy_generation) = graph_retry_fixture().await;
    let busy_providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: Some(busy_generation.generation.clone()),
    };
    let busy = run_retry(&busy_ledger, &busy_providers, &busy_generation.generation).await;
    assert_eq!(busy.resolved, 0);
    assert_eq!(
        busy.failed, 1,
        "finalization cannot pretend an active source lease is available"
    );
    assert_eq!(busy_providers.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        busy_ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .len(),
        3
    );
}

#[path = "finalizer_tests.rs"]
mod finalizer_tests;

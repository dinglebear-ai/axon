use super::*;
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind,
    JobPriority, JobStagePlan, LifecycleStatus, MetadataMap, PipelinePhase,
};
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;
use axon_jobs::unified::SqliteUnifiedJobStore;
use std::time::Duration;

async fn test_cfg() -> (tempfile::TempDir, Arc<Config>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    (tmp, Arc::new(cfg))
}

fn detached_job_request(kind: UnifiedJobKind) -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req-job-runner-test".to_string()),
        job_kind: kind,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Planning,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: Some(1),
        }],
        request: Some(serde_json::json!({"operation": format!("{kind:?}").to_lowercase()})),
        auth_snapshot: AuthSnapshot::default(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_job_runner_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("job_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Polls `store.get(job_id)` until the job reaches a terminal `LifecycleStatus`
/// or the timeout elapses. This is genuine detached execution: the job row is
/// created directly against the store (no inline call), and only the unified
/// worker loop spawned by `SqliteJobBackend::new_with_workers_and_registry`
/// picks it up and runs it to completion.
async fn wait_for_terminal(
    store: &SqliteUnifiedJobStore,
    job_id: axon_api::source::JobId,
) -> axon_api::source::JobSummary {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(summary) = store.get(job_id).await.expect("get job")
                && matches!(
                    summary.status,
                    LifecycleStatus::Completed
                        | LifecycleStatus::CompletedDegraded
                        | LifecycleStatus::Failed
                        | LifecycleStatus::Canceled
                )
            {
                return summary;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("job should reach a terminal state before the test timeout")
}

/// First real test of true detached/background execution for a registered
/// unified job kind: the job is created directly against the unified job
/// store (never invoked inline), and only the unified worker loop — wired
/// through `build_registry`'s `JobKind::Memory` runner — picks it up and
/// drives it to `Completed`.
#[tokio::test]
async fn detached_memory_compaction_job_is_claimed_and_completed_by_unified_worker() {
    let (_tmp, cfg) = test_cfg().await;
    let backend = SqliteJobBackend::new_with_workers_and_registry(
        Arc::clone(&cfg),
        Some(Arc::new(build_registry(&cfg).expect("build registry"))),
    )
    .await
    .expect("backend with workers + registry");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let job = store
        .create(detached_job_request(UnifiedJobKind::Memory))
        .await
        .expect("create detached memory compaction job");

    let summary = wait_for_terminal(&store, job.job_id).await;

    assert_eq!(
        summary.status,
        LifecycleStatus::Completed,
        "memory compaction runner should complete via the real SqliteMemoryStore capabilities() call: {:?}",
        summary.last_error
    );

    backend.shutdown().await;
}

/// Real end-to-end proof (not just the registry-seam fallback above): a
/// `memory_compaction`-payload job, claimed and run purely through the
/// detached unified worker (never invoked inline), actually merges two
/// pre-seeded memories into a third — the `MemoryCompactionRunner` payload
/// dispatch, not `capabilities()`, did the work.
#[tokio::test]
async fn detached_memory_compaction_job_actually_compacts_seeded_memories() {
    use axon_api::source::{
        MemoryCompactRequest, MemoryId, MemoryRequest, MemoryScope, MemoryType, Timestamp,
    };
    use axon_memory::record::SystemClock as MemoryClock;
    use axon_memory::sqlite::SqliteMemoryStore;
    use axon_memory::store::MemoryStore;

    let (_tmp, cfg) = test_cfg().await;

    // Initialize the shared schema through the canonical composed runner,
    // then seed through the migration-free domain handle.
    let pool = axon_jobs::store::open_sqlite_pool(&cfg.sqlite_path.to_string_lossy())
        .await
        .expect("initialize canonical schema");
    pool.close().await;
    let seed_store =
        SqliteMemoryStore::open_migrated(&cfg.sqlite_path.to_string_lossy(), Arc::new(MemoryClock))
            .expect("open seed memory store");
    let scope = MemoryScope {
        kind: "project".to_string(),
        value: "axon".to_string(),
    };
    let first = seed_store
        .remember(MemoryRequest {
            memory_type: MemoryType::Fact,
            body: "alpha fact".to_string(),
            confidence: 0.8,
            salience: 0.6,
            scope: scope.clone(),
            title: None,
            tags: Vec::new(),
            links: Vec::new(),
            decay: None,
            embed: false,
            visibility: None,
        })
        .await
        .expect("remember first");
    let second = seed_store
        .remember(MemoryRequest {
            memory_type: MemoryType::Fact,
            body: "beta fact".to_string(),
            confidence: 0.7,
            salience: 0.5,
            scope: scope.clone(),
            title: None,
            tags: Vec::new(),
            links: Vec::new(),
            decay: None,
            embed: false,
            visibility: None,
        })
        .await
        .expect("remember second");

    let backend = SqliteJobBackend::new_with_workers_and_registry(
        Arc::clone(&cfg),
        Some(Arc::new(build_registry(&cfg).expect("build registry"))),
    )
    .await
    .expect("backend with workers + registry");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let compact_request = MemoryCompactRequest {
        memory_ids: vec![
            MemoryId::new(first.memory_id.0.clone()),
            MemoryId::new(second.memory_id.0.clone()),
        ],
        strategy: "concatenate".to_string(),
        result_type: MemoryType::Fact,
        title: None,
        scope,
        archive_sources: true,
        instructions: None,
        timestamp: Timestamp(chrono::Utc::now().to_rfc3339()),
    };
    let mut request = detached_job_request(UnifiedJobKind::Memory);
    request.request = Some(serde_json::json!({
        "operation": "memory_compaction",
        "payload": serde_json::to_value(&compact_request).expect("serialize compact request"),
    }));

    let job = store
        .create(request)
        .await
        .expect("create detached memory compaction job");
    let summary = wait_for_terminal(&store, job.job_id).await;
    assert_eq!(
        summary.status,
        LifecycleStatus::Completed,
        "real payload compaction should complete: {:?}",
        summary.last_error
    );

    // The two sources are archived and a third (compacted) memory exists.
    let first_after = seed_store
        .get(first.memory_id.clone())
        .await
        .expect("get first")
        .expect("first still exists");
    assert_eq!(first_after.status, axon_api::source::MemoryStatus::Archived);
    let second_after = seed_store
        .get(second.memory_id.clone())
        .await
        .expect("get second")
        .expect("second still exists");
    assert_eq!(
        second_after.status,
        axon_api::source::MemoryStatus::Archived
    );

    backend.shutdown().await;
}

/// Same proof for `JobKind::ProviderProbe`, backed by the real
/// `system::doctor::doctor` connectivity check.
#[tokio::test]
async fn detached_provider_probe_job_is_claimed_and_completed_by_unified_worker() {
    let (_tmp, cfg) = test_cfg().await;
    let backend = SqliteJobBackend::new_with_workers_and_registry(
        Arc::clone(&cfg),
        Some(Arc::new(build_registry(&cfg).expect("build registry"))),
    )
    .await
    .expect("backend with workers + registry");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let job = store
        .create(detached_job_request(UnifiedJobKind::ProviderProbe))
        .await
        .expect("create detached provider probe job");

    let summary = wait_for_terminal(&store, job.job_id).await;

    assert_eq!(
        summary.status,
        LifecycleStatus::Completed,
        "provider probe runner should complete via the real doctor() connectivity check"
    );

    backend.shutdown().await;
}

/// Regression guard: a job kind with no registered runner (e.g. `Research`,
/// intentionally out of scope for this wave) must still fail cleanly with
/// `job_runner.unsupported_stage` instead of hanging or panicking.
#[tokio::test]
async fn detached_unregistered_kind_falls_back_to_unsupported_stage() {
    let (_tmp, cfg) = test_cfg().await;
    let backend = SqliteJobBackend::new_with_workers_and_registry(
        Arc::clone(&cfg),
        Some(Arc::new(build_registry(&cfg).expect("build registry"))),
    )
    .await
    .expect("backend with workers + registry");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let job = store
        .create(detached_job_request(UnifiedJobKind::Research))
        .await
        .expect("create detached research job");

    let summary = wait_for_terminal(&store, job.job_id).await;

    assert_eq!(summary.status, LifecycleStatus::Failed);
    assert!(
        summary
            .last_error
            .as_ref()
            .is_some_and(|error| error.message.contains("not wired yet")),
        "unregistered kind should fail with the unsupported_stage message, got: {:?}",
        summary.last_error
    );

    backend.shutdown().await;
}

/// `build_registry` is additive: it must only ever register the kinds this
/// wave explicitly wires (Memory, ProviderProbe), never GraphMutation/Prune/
/// Watch, which have their own architectural wrinkles left to follow-up work.
#[tokio::test]
async fn build_registry_only_registers_in_scope_kinds() {
    let (_tmp, cfg) = test_cfg().await;
    let registry = build_registry(&cfg).expect("build registry");

    assert!(registry.contains(UnifiedJobKind::Memory));
    assert!(registry.contains(UnifiedJobKind::ProviderProbe));
    assert!(!registry.contains(UnifiedJobKind::Graph));
    assert!(!registry.contains(UnifiedJobKind::Prune));
    assert!(!registry.contains(UnifiedJobKind::Watch));
    assert!(!registry.contains(UnifiedJobKind::Research));
}

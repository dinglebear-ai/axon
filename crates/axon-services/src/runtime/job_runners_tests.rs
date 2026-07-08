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

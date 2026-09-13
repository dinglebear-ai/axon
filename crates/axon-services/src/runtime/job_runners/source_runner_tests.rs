use super::*;
use crate::runtime::job_runners::build_registry;
use axon_api::source::{
    AuthScope, AuthSnapshot, ConfigSnapshotId, JobCreateRequest, JobIntent,
    JobKind as UnifiedJobKind, JobPriority, JobStagePlan, JobStatusUpdate, LifecycleStatus,
    MetadataMap, PipelinePhase, StageCounts,
};
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;

#[tokio::test(start_paused = true)]
async fn heartbeat_waiting_for_writer_must_keep_polling_source() {
    let gate = SqliteWriteGate::default();
    let shutdown = CancellationToken::new();
    let source = async {
        let _writer = gate.lock().await;
        tokio::time::sleep(std::time::Duration::from_secs(31)).await;
        42
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(35),
        drive_source_with_heartbeat(
            source,
            &shutdown,
            std::time::Duration::from_secs(30),
            || async {
                let _writer = gate.lock().await;
                true
            },
        ),
    )
    .await
    .expect("heartbeat must not suspend the source holding its writer gate")
    .expect("source completes");
    assert_eq!(result, 42);
}

#[tokio::test(start_paused = true)]
async fn short_watchdog_heartbeat_cadence_ticks_during_long_work() {
    let shutdown = CancellationToken::new();
    let heartbeats = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = Arc::clone(&heartbeats);
    let source = async {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        42
    };
    let result =
        drive_source_with_heartbeat(source, &shutdown, std::time::Duration::from_secs(1), || {
            let observed = Arc::clone(&observed);
            async move {
                observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                true
            }
        })
        .await
        .expect("source completes");
    assert_eq!(result, 42);
    assert!(heartbeats.load(std::sync::atomic::Ordering::SeqCst) >= 3);
}

#[tokio::test(start_paused = true)]
async fn cancellation_bounds_cleanup_while_heartbeat_is_blocked() {
    let shutdown = CancellationToken::new();
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_secs(31)).await;
        shutdown.cancel();
    };
    let running = drive_source_with_heartbeat(
        std::future::pending::<()>(),
        &shutdown,
        std::time::Duration::from_secs(30),
        std::future::pending::<bool>,
    );
    let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(42), async {
        tokio::join!(cancel, running)
    })
    .await
    .expect("a blocked heartbeat must not hide cancellation or its cleanup deadline");
    assert!(result.is_err());
}

#[tokio::test(start_paused = true)]
async fn cancellation_releases_heartbeat_writer_before_source_cleanup() {
    let gate = SqliteWriteGate::default();
    let shutdown = CancellationToken::new();
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_secs(31)).await;
        shutdown.cancel();
    };
    let source = async {
        shutdown.cancelled().await;
        let _writer = gate.lock().await;
        42
    };
    let running = drive_source_with_heartbeat(
        source,
        &shutdown,
        std::time::Duration::from_secs(30),
        || async {
            let _writer = gate.lock().await;
            std::future::pending::<bool>().await
        },
    );
    let (_, result) = tokio::join!(cancel, running);
    assert_eq!(
        result.expect("cleanup must acquire the abandoned heartbeat writer"),
        42
    );
    assert!(
        gate.try_lock().is_some(),
        "all writer guards must be released"
    );
}

async fn test_cfg() -> (tempfile::TempDir, Arc<Config>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    // Explicitly unset — asserts these tests exercise the no-data-plane
    // degraded path deterministically, independent of any ambient
    // QDRANT_URL/TEI_URL env the process happens to inherit.
    cfg.qdrant_url = String::new();
    cfg.tei_url = String::new();
    (tmp, Arc::new(cfg))
}

fn source_job_request(source_request: &SourceRequest) -> JobCreateRequest {
    JobCreateRequest {
        request_id: Some("req-source-runner-test".to_string()),
        job_kind: UnifiedJobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![
            JobStagePlan::required(PipelinePhase::Fetching).with_estimated_items(Some(1)),
        ],
        request: Some(serde_json::json!({
            "source_request": source_request,
        })),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_source_runner_test")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Build a claimed job + enqueue-only store for direct `SourceRunner::run`
/// invocation.
///
/// Tests in this file call `SourceRunner::run` directly rather than going
/// through `SqliteJobBackend::new_with_workers_and_registry` +
/// `build_registry`. `build_registry` opens its own `SqliteMemoryStore`
/// (rusqlite, synchronous `ensure_schema`) against the same fresh sqlite file
/// *before* the async `SqliteJobBackend` construction gets a chance to run
/// the composed sqlx migration set — this pre-existing ordering (present on
/// `main`, independent of this runner) makes the composed `memory/0003`
/// migration hit "duplicate column name: visibility" and fails backend
/// construction outright for every job kind's `new_with_workers_and_registry`
/// test in this crate, not just `Source`'s. Driving `SourceRunner::run`
/// directly against a plain enqueue-only `SqliteJobBackend::new` (no
/// `build_registry` involved) sidesteps that unrelated construction bug and
/// still exercises the runner's real logic end-to-end: real payload
/// deserialization, a real lazily-built `ServiceContext` sharing the worker
/// pool, and a real
/// `crate::source::index_source_with_auth` call.
async fn claim_source_job(
    store: &SqliteUnifiedJobStore,
    request_payload: serde_json::Value,
) -> UnifiedClaimedJob {
    let mut create_request = source_job_request(&SourceRequest::new("placeholder"));
    create_request.request = Some(request_payload.clone());
    let created = store
        .create(create_request)
        .await
        .expect("create source job row");
    UnifiedClaimedJob {
        job_id: created.job_id,
        kind: UnifiedJobKind::Source,
        attempt: 1,
        request_json: Some(request_payload),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
    }
}

/// (a) A claimed `Source` job actually runs the pipeline and reaches a
/// terminal status instead of hanging `running`/`queued` forever — the core
/// C4-02 / bead `axon_rust-mijoc` regression this runner fixes. With no
/// Qdrant/TEI configured, `index_source_with_auth` degrades cleanly to a
/// `Failed` `SourceResult` (data plane unconfigured) rather than erroring the
/// call outright, so this also proves the runner correctly propagates that
/// `Ok(SourceResult{status: Failed, ..})` into a real `Err(ApiError)` (which
/// the unified worker loop then turns into a job-store `Failed` transition)
/// rather than mis-reporting it as `Ok(())`/`Completed`.
#[tokio::test]
async fn source_runner_reaches_terminal_failed_without_data_plane() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::new("https://example.com/docs/getting-started");
    let claimed = claim_source_job(&store, serde_json::json!({"source_request": request})).await;

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err(
        "source job with no data plane configured must return Err, not Ok, so the unified \
         worker marks it Failed instead of Completed",
    );
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("data plane"),
        "expected the data_plane_unconfigured warning surfaced as the runner's ApiError, got: {}",
        error.message
    );
}

/// (b) A malformed/unroutable source request fails the job with a
/// descriptive `ApiError` (distinct failure mode from the data-plane
/// degradation above — this one never reaches `index_source_with_auth`'s
/// dispatch at all, it fails in the runner's own payload validation).
#[tokio::test]
async fn source_runner_with_missing_payload_fails_with_api_error() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let claimed = claim_source_job(&store, serde_json::json!({"not_a_source_request": true})).await;

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err("missing `source_request` payload must fail the job");
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("source_request"),
        "expected a validation error naming the missing field, got: {}",
        error.message
    );
}

#[tokio::test]
async fn source_runner_rechecks_local_scope_from_persisted_snapshot() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::local_path("/tmp/axon-missing-runner-local-auth-test", true);
    let mut claimed =
        claim_source_job(&store, serde_json::json!({"source_request": request})).await;
    claimed.auth_snapshot = AuthSnapshot::default();
    claimed.auth_snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write];

    let shutdown = CancellationToken::new();
    let result = runner.run(&claimed, &store, &shutdown).await;

    let error = result.expect_err("worker-local source without local scope must fail the job");
    assert_eq!(error.code.0, "job_runner.source_failed");
    assert!(
        error.message.contains("axon:local"),
        "expected source runner to surface the service auth recheck, got: {}",
        error.message
    );
}

/// (c) Cancellation before the runner starts executing is honored: the
/// runner's own `shutdown.is_cancelled()` guard (mirroring every other
/// unified runner — Crawl/Embed/Extract/Ingest) short-circuits before doing
/// any work, rather than starting `index_source_with_auth` after shutdown was
/// requested. Exercised directly against `SourceRunner::run` (not the full
/// claim loop) so the assertion is deterministic instead of racing a real
/// shutdown against the worker's claim/spawn timing.
#[tokio::test]
async fn source_runner_honors_cancellation_before_running() {
    let (_tmp, cfg) = test_cfg().await;
    let runner = SourceRunner::new(Arc::clone(&cfg));

    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());

    let request = SourceRequest::new("https://example.com/docs");
    let created = store
        .create(source_job_request(&request))
        .await
        .expect("create source job row");

    let claimed = UnifiedClaimedJob {
        job_id: created.job_id,
        kind: UnifiedJobKind::Source,
        attempt: 1,
        request_json: Some(serde_json::json!({"source_request": request})),
        auth_snapshot: AuthSnapshot::trusted_system("test"),
    };

    let shutdown = CancellationToken::new();
    shutdown.cancel();

    let result = runner.run(&claimed, &store, &shutdown).await;
    let error = result.expect_err("canceled-before-running must return Err, not proceed");
    assert!(
        error.message.contains("canceled before running"),
        "expected the pre-flight cancellation message, got: {}",
        error.message
    );
}

#[tokio::test]
async fn periodic_heartbeat_preserves_durable_phase_and_counts() {
    let (_tmp, cfg) = test_cfg().await;
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let store = SqliteUnifiedJobStore::new(Arc::clone(backend.pool()).as_ref().clone());
    let request = SourceRequest::new("https://example.com/docs");
    let claimed = claim_source_job(&store, serde_json::json!({"source_request": request})).await;
    let expected = StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: Some(2),
        documents_done: 1,
        chunks_total: Some(24),
        chunks_done: 12,
        bytes_total: None,
        bytes_done: 0,
    };
    store
        .update_status(JobStatusUpdate {
            job_id: claimed.job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Embedding,
            stage_id: None,
            counts: Some(expected.clone()),
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("advance job progress");

    crate::runtime::job_runners::heartbeat_running_preserving_progress(&store, &claimed).await;

    let summary = store
        .get(claimed.job_id)
        .await
        .expect("get job")
        .expect("job exists");
    assert_eq!(summary.phase, PipelinePhase::Embedding);
    assert_eq!(summary.counts, Some(expected));
}

/// `build_registry` wires Source as the acquisition/indexing family.
#[tokio::test]
async fn build_registry_registers_source() {
    let (_tmp, cfg) = test_cfg().await;
    let registry = build_registry(&cfg).expect("build registry");
    assert!(registry.contains(UnifiedJobKind::Source));
}

#[tokio::test]
async fn source_runner_context_reuses_the_worker_pool() {
    let (_tmp, cfg) = test_cfg().await;
    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .expect("enqueue-only backend");
    let worker_pool = Arc::clone(backend.pool());
    let store = SqliteUnifiedJobStore::new(worker_pool.as_ref().clone());

    let write_gate = axon_jobs::scheduler::SqliteWriteGate::default();
    let ctx = build_service_context_with_write_gate(&cfg, &store, write_gate.clone())
        .await
        .expect("runner service context");
    let context_pool = ctx.jobs.sqlite_pool().expect("runner SQLite pool");

    let mut held = Vec::new();
    for _ in 0..worker_pool.options().get_max_connections() {
        held.push(worker_pool.acquire().await.expect("worker pool connection"));
    }
    assert!(
        context_pool.try_acquire().is_none(),
        "the source runner must reuse the worker's SQLx pool instead of opening a second pool"
    );
    drop(held);

    let _held_gate = write_gate.lock().await;
    assert!(
        ctx.jobs
            .sqlite_write_gate()
            .expect("runner writer gate")
            .try_lock()
            .is_none(),
        "the source runner must reuse the process writer gate"
    );
}

use super::*;

use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use axon_api::source::{
    JobCancelRequest, JobCreateRequest, JobIntent, JobPriority, JobRecoveryRequest, JobStagePlan,
    LifecycleStatus, MetadataMap, Timestamp,
};
use tempfile::NamedTempFile;
use tokio::sync::Notify;

async fn test_pool() -> (SqlitePool, NamedTempFile) {
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    (pool, temp)
}

async fn enqueue_test_job(pool: &SqlitePool, kind: UnifiedJobKind) -> JobId {
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
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
                phase: PipelinePhase::Fetching,
                required: true,
                provider_requirements: Vec::new(),
                estimated_items: None,
            }],
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: None,
            requirements: MetadataMap::new(),
            result_schema: None,
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .unwrap();
    descriptor.job_id
}

/// Runner whose `run` panics mid-execution — used to prove
/// `run_unified_claimed`'s panic guard catches the unwind and marks the job
/// `failed` rather than leaving it stuck `running` forever.
struct PanickingRunner;

#[async_trait::async_trait]
impl UnifiedJobRunner for PanickingRunner {
    async fn run(
        &self,
        _claimed: &UnifiedClaimedJob,
        _store: &SqliteUnifiedJobStore,
        _shutdown: &CancellationToken,
    ) -> Result<UnifiedJobOutcome, ApiError> {
        panic!("boom: simulated runner panic");
    }
}

#[tokio::test]
async fn panicking_runner_marks_job_failed_not_stuck_running() {
    let (pool, _temp) = test_pool().await;
    let job_id = enqueue_test_job(&pool, UnifiedJobKind::Memory).await;

    let claimed = claim_next_unified_job(&pool)
        .await
        .unwrap()
        .expect("job should be claimable");
    assert_eq!(claimed.job_id, job_id);

    let mut registry = JobRunnerRegistry::new();
    registry.register(UnifiedJobKind::Memory, Arc::new(PanickingRunner));
    let registry = Arc::new(registry);

    let shutdown = CancellationToken::new();
    // Must not propagate the panic to the test task — the guard inside
    // run_unified_claimed is expected to catch it.
    run_unified_claimed(&pool, &claimed, &shutdown, Some(&registry)).await;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let summary = store.get(job_id).await.unwrap().expect("job must exist");
    assert_eq!(
        summary.status,
        LifecycleStatus::Failed,
        "job must be marked failed after a runner panic, not left running"
    );
    assert!(
        summary.last_error.is_some(),
        "a failed-by-panic job should carry an error explaining why"
    );
}

#[tokio::test]
async fn healthy_runner_still_marks_job_completed() {
    let (pool, _temp) = test_pool().await;
    let job_id = enqueue_test_job(&pool, UnifiedJobKind::Memory).await;

    struct OkRunner;
    #[async_trait::async_trait]
    impl UnifiedJobRunner for OkRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            _shutdown: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            Ok(UnifiedJobOutcome::completed_without_counts())
        }
    }

    let claimed = claim_next_unified_job(&pool).await.unwrap().unwrap();
    let mut registry = JobRunnerRegistry::new();
    registry.register(UnifiedJobKind::Memory, Arc::new(OkRunner));
    let registry = Arc::new(registry);

    let shutdown = CancellationToken::new();
    run_unified_claimed(&pool, &claimed, &shutdown, Some(&registry)).await;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let summary = store.get(job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Completed);
}

#[tokio::test]
async fn runner_completion_counts_survive_the_terminal_transition() {
    let (pool, _temp) = test_pool().await;
    let job_id = enqueue_test_job(&pool, UnifiedJobKind::Source).await;
    let counts = axon_api::source::StageCounts {
        items_total: Some(344),
        items_done: 344,
        documents_total: Some(344),
        documents_done: 344,
        chunks_total: Some(7_608),
        chunks_done: 7_608,
        bytes_total: None,
        bytes_done: 0,
    };

    struct CountsRunner {
        counts: axon_api::source::StageCounts,
    }
    #[async_trait::async_trait]
    impl UnifiedJobRunner for CountsRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            _shutdown: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            Ok(UnifiedJobOutcome::completed(self.counts.clone()))
        }
    }

    let claimed = claim_next_unified_job(&pool).await.unwrap().unwrap();
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Source,
        Arc::new(CountsRunner {
            counts: counts.clone(),
        }),
    );
    let registry = Arc::new(registry);

    run_unified_claimed(&pool, &claimed, &CancellationToken::new(), Some(&registry)).await;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let summary = store.get(job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Completed);
    assert_eq!(summary.counts, Some(counts));
}

#[tokio::test]
async fn running_cancel_reaches_runner_and_cannot_be_overwritten_by_completion() {
    struct CancelAwareRunner {
        started: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl UnifiedJobRunner for CancelAwareRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            cancel: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            self.started.notify_one();
            cancel.cancelled().await;
            Ok(UnifiedJobOutcome::completed_without_counts())
        }
    }

    let (pool, _temp) = test_pool().await;
    let job_id = enqueue_test_job(&pool, UnifiedJobKind::Memory).await;
    let claimed = claim_next_unified_job(&pool).await.unwrap().unwrap();
    let started = Arc::new(Notify::new());
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Memory,
        Arc::new(CancelAwareRunner {
            started: Arc::clone(&started),
        }),
    );
    let registry = Arc::new(registry);
    let worker_pool = pool.clone();
    let handle = tokio::spawn(async move {
        run_unified_claimed(
            &worker_pool,
            &claimed,
            &CancellationToken::new(),
            Some(&registry),
        )
        .await;
    });
    started.notified().await;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let canceled = store
        .cancel(
            job_id,
            JobCancelRequest {
                reason: Some("test cancellation".to_string()),
                force_after_ms: None,
                actor: Some("test".to_string()),
            },
        )
        .await
        .unwrap();
    assert_eq!(canceled.status, LifecycleStatus::Canceling);
    handle.await.unwrap();

    let summary = store.get(job_id).await.unwrap().unwrap();
    assert_eq!(summary.status, LifecycleStatus::Canceled);
    assert_eq!(summary.phase, PipelinePhase::Canceled);
}

/// Runner that tracks how many instances of itself are executing
/// concurrently (peak observed), then sleeps briefly before completing —
/// long enough that overlapping claims would show up as concurrency > 1 if
/// the source-specific gate were not enforced.
struct ConcurrencyTrackingRunner {
    current: Arc<std::sync::atomic::AtomicUsize>,
    peak: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl UnifiedJobRunner for ConcurrencyTrackingRunner {
    async fn run(
        &self,
        _claimed: &UnifiedClaimedJob,
        _store: &SqliteUnifiedJobStore,
        _shutdown: &CancellationToken,
    ) -> Result<UnifiedJobOutcome, ApiError> {
        use std::sync::atomic::Ordering;
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        self.current.fetch_sub(1, Ordering::SeqCst);
        Ok(UnifiedJobOutcome::completed_without_counts())
    }
}

/// Regression test for fix 3: source jobs must stay bounded by the general
/// source-job concurrency limit even when the general
/// `unified_worker_concurrency` semaphore is set much higher. This is a
/// general per-source-kind cap, not a web/CDP-specific one — several source
/// kinds share constrained external resources (a single Chrome instance for
/// web/render-backed acquisition, upstream API rate limits for other
/// adapters), so letting them freely consume general worker slots risks
/// starving other job kinds or exhausting a shared resource.
#[tokio::test]
async fn source_jobs_stay_bounded_by_source_specific_limit_even_with_high_general_concurrency() {
    let (pool, _temp) = test_pool().await;
    let pool = Arc::new(pool);

    const SOURCE_JOBS: usize = 4;
    for _ in 0..SOURCE_JOBS {
        enqueue_test_job(&pool, UnifiedJobKind::Source).await;
    }

    let current = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Source,
        Arc::new(ConcurrencyTrackingRunner {
            current: Arc::clone(&current),
            peak: Arc::clone(&peak),
        }),
    );
    let registry = Arc::new(registry);

    let notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();

    // High general concurrency (8), but a tightly-set source-specific limit
    // of 1 (independent of Config::source_job_concurrency_limit's own
    // default of 4) — the tight value, not the specific number, is what
    // proves the semaphore actually bounds source jobs below general
    // concurrency.
    let handle = tokio::spawn(unified_worker_loop_with_concurrency_limits(
        Arc::clone(&pool),
        Arc::clone(&notify),
        shutdown.clone(),
        Some(registry),
        8,
        1,
    ));

    // Poll until every source job has reached a terminal state (bounded so a
    // regression hangs the test instead of looping forever). Re-notify on
    // every poll tick rather than relying on a single notify_one() racing the
    // worker task's startup and first select! registration — the fallback
    // POLL_INTERVAL is 5s, so a lost single notify would otherwise make this
    // test flaky under load rather than a real regression.
    let store = SqliteUnifiedJobStore::new((*pool).clone());
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            notify.notify_one();
            let page = store
                .list(axon_api::source::JobListRequest {
                    kind: Some(UnifiedJobKind::Source),
                    status: None,
                    source_id: None,
                    watch_id: None,
                    limit: Some(SOURCE_JOBS as u32),
                    cursor: None,
                })
                .await
                .unwrap();
            if page.items.len() == SOURCE_JOBS
                && page
                    .items
                    .iter()
                    .all(|job| job.status == LifecycleStatus::Completed)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("all source jobs should complete within 20s");

    shutdown.cancel();
    let _ = handle.await;

    assert_eq!(
        peak.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "at most one source job should ever run concurrently despite concurrency=8"
    );
}

/// Regression test for defect P1: a `Source` job that cannot get the
/// source-specific permit right now must never block the claim loop from
/// claiming and running jobs of a *different* kind. Enqueues more `Source`
/// jobs than the general concurrency allows to even be claimed at once (the
/// exact "N+1 queued source jobs, N = general concurrency" shape from the
/// bug report), plus one `Prune` job queued behind them, and asserts the
/// `Prune` job completes while the `Source` jobs are still deliberately
/// parked — i.e. without ever waiting for the source lane to drain.
///
/// Before the fix: the general permit was acquired *after* a job was
/// already claimed (flipped to `running`) and the source-specific permit was
/// then awaited *inside* the spawned task, so a parked `Source` task held a
/// general-concurrency slot hostage. With `concurrency = 2` and
/// `source_concurrency = 1`, the third `Source` claim exhausts the general
/// semaphore (one task running, one task parked-but-holding-a-permit) and
/// the claim loop blocks forever waiting for a general permit before it can
/// even look at the `Prune` job — this test times out and fails on
/// unfixed code.
#[tokio::test]
async fn non_source_job_completes_without_waiting_for_source_jobs_to_drain() {
    let (pool, _temp) = test_pool().await;
    let pool = Arc::new(pool);

    const GENERAL_CONCURRENCY: usize = 2;
    const SOURCE_CONCURRENCY: usize = 1;
    const SOURCE_JOBS: usize = 3; // > GENERAL_CONCURRENCY, matching "N+1" from the bug report

    let mut source_job_ids = Vec::new();
    for _ in 0..SOURCE_JOBS {
        source_job_ids.push(enqueue_test_job(&pool, UnifiedJobKind::Source).await);
    }
    let prune_job_id = enqueue_test_job(&pool, UnifiedJobKind::Prune).await;

    struct BlockingRunner {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }
    #[async_trait::async_trait]
    impl UnifiedJobRunner for BlockingRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            _shutdown: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(UnifiedJobOutcome::completed_without_counts())
        }
    }

    struct ImmediateRunner;
    #[async_trait::async_trait]
    impl UnifiedJobRunner for ImmediateRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            _shutdown: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            Ok(UnifiedJobOutcome::completed_without_counts())
        }
    }

    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Source,
        Arc::new(BlockingRunner {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        }),
    );
    registry.register(UnifiedJobKind::Prune, Arc::new(ImmediateRunner));
    let registry = Arc::new(registry);

    let notify = Arc::new(Notify::new());
    let activity = Arc::new(WorkerActivity::default());
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(unified_worker_loop_with_concurrency_limits_and_activity(
        Arc::clone(&pool),
        Arc::clone(&notify),
        Arc::clone(&activity),
        shutdown.clone(),
        Some(registry),
        GENERAL_CONCURRENCY,
        SOURCE_CONCURRENCY,
    ));

    // Wait until a Source job has actually started (parked on `release`), so
    // we know the source lane is genuinely occupied before checking on the
    // Prune job below.
    tokio::time::timeout(std::time::Duration::from_secs(10), started.notified())
        .await
        .expect("a source job should start within 10s");
    assert!(
        activity.in_flight() > 0,
        "a claimed runner must remain visible to process-level drain tracking"
    );

    let store = SqliteUnifiedJobStore::new((*pool).clone());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            notify.notify_one();
            let summary = store.get(prune_job_id).await.unwrap().unwrap();
            if summary.status == LifecycleStatus::Completed {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect(
        "the Prune job must complete without waiting for the Source jobs \
         (still deliberately parked on `release`) to drain",
    );

    // Sanity: prove this test actually exercised the starvation scenario —
    // the source jobs must still be blocked (not completed) when the Prune
    // job finished, rather than having raced ahead of it.
    for job_id in &source_job_ids {
        let summary = store.get(*job_id).await.unwrap().unwrap();
        assert_ne!(
            summary.status,
            LifecycleStatus::Completed,
            "source jobs should still be parked on `release` when the Prune job completes"
        );
    }

    release.notify_waiters();
    shutdown.cancel();
    let _ = handle.await;
    assert_eq!(
        activity.in_flight(),
        0,
        "drain tracking must clear only after every runner returns"
    );
}

/// Regression test for defect P2: a job must never sit `status = 'running'`
/// while its task cannot yet make progress (parked behind a full
/// source-specific lane) — that state is exactly what let the watchdog's
/// stale-job sweep reclaim a still-alive job and requeue it for a second,
/// duplicate execution. With the P1 fix, a job that cannot get the
/// source-specific permit is left `queued` (never claimed) until a worker is
/// genuinely about to run it, so it can never become eligible for
/// `recover_jobs`'s `status IN ('running', 'waiting')` sweep while parked.
///
/// This directly exercises `JobStore::recover` (the same call the watchdog
/// makes — see `crates/axon-jobs/src/workers/watchdog.rs`) with an
/// aggressive `stale_before` cutoff of "right now" while a second `Source`
/// job is blocked behind a full source lane, and asserts recovery finds
/// nothing to reclaim.
#[tokio::test]
async fn job_parked_behind_full_source_lane_is_not_stale_recoverable() {
    let (pool, _temp) = test_pool().await;
    let pool = Arc::new(pool);

    let running_job_id = enqueue_test_job(&pool, UnifiedJobKind::Source).await;
    let parked_job_id = enqueue_test_job(&pool, UnifiedJobKind::Source).await;

    struct BlockingRunner {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }
    #[async_trait::async_trait]
    impl UnifiedJobRunner for BlockingRunner {
        async fn run(
            &self,
            _claimed: &UnifiedClaimedJob,
            _store: &SqliteUnifiedJobStore,
            _shutdown: &CancellationToken,
        ) -> Result<UnifiedJobOutcome, ApiError> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(UnifiedJobOutcome::completed_without_counts())
        }
    }

    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Source,
        Arc::new(BlockingRunner {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        }),
    );
    let registry = Arc::new(registry);

    let notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();
    // source_concurrency = 1: only one of the two Source jobs can run.
    let handle = tokio::spawn(unified_worker_loop_with_concurrency_limits(
        Arc::clone(&pool),
        Arc::clone(&notify),
        shutdown.clone(),
        Some(registry),
        8,
        1,
    ));

    tokio::time::timeout(std::time::Duration::from_secs(10), started.notified())
        .await
        .expect("the first source job should start within 10s");

    let store = SqliteUnifiedJobStore::new((*pool).clone());
    // Give the parked job every chance to have been (incorrectly) claimed if
    // the fix regressed, without depending on a specific delay elsewhere.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let parked_summary = store.get(parked_job_id).await.unwrap().unwrap();
    assert_eq!(
        parked_summary.status,
        LifecycleStatus::Queued,
        "a job that cannot get the source-specific permit must stay queued, \
         never flip to running with nowhere to run"
    );

    // The watchdog's own sweep uses exactly this call
    // (crates/axon-jobs/src/workers/watchdog.rs) with a cutoff derived from
    // `watchdog_stale_timeout_secs + watchdog_confirm_secs` (360s by
    // default). Use a generous-but-realistic 5-minute cutoff here — not
    // "right now", which would also flag the genuinely-running job's
    // legitimately-fresh (merely non-zero-age) heartbeat as "before the
    // cutoff" and defeat the point of the assertion — to prove there is
    // nothing actually stale for it to find.
    let stale_before = Timestamp::from(chrono::Utc::now() - chrono::Duration::seconds(5 * 60));
    let recovery = store
        .recover(JobRecoveryRequest {
            kind: None,
            stale_before: Some(stale_before),
            limit: None,
            older_than_seconds: None,
            dry_run: false,
            allow_without_cutoff: false,
        })
        .await
        .unwrap();
    assert_eq!(
        recovery.jobs_requeued, 0,
        "no job should be reclaimed: the running job has a fresh heartbeat \
         and the parked job was never claimed in the first place"
    );

    let parked_summary_after = store.get(parked_job_id).await.unwrap().unwrap();
    assert_eq!(
        parked_summary_after.attempt, 1,
        "the parked job's attempt count must be untouched by recovery"
    );
    let running_summary = store.get(running_job_id).await.unwrap().unwrap();
    assert_eq!(
        running_summary.status,
        LifecycleStatus::Running,
        "the actually-running job must not have been reclaimed either"
    );

    release.notify_waiters();
    shutdown.cancel();
    let _ = handle.await;
}

#[test]
fn panic_message_extracts_str_and_string_payloads() {
    let str_payload: Box<dyn std::any::Any + Send> = Box::new("literal panic");
    assert_eq!(panic_message(str_payload.as_ref()), "literal panic");

    let string_payload: Box<dyn std::any::Any + Send> = Box::new(String::from("owned panic"));
    assert_eq!(panic_message(string_payload.as_ref()), "owned panic");

    let other_payload: Box<dyn std::any::Any + Send> = Box::new(42_i32);
    assert_eq!(
        panic_message(other_payload.as_ref()),
        "non-string panic payload"
    );
}

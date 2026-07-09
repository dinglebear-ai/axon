use super::*;

use crate::boundary::JobStore;
use crate::store::open_sqlite_pool;
use axon_api::source::{JobCreateRequest, JobIntent, JobPriority, JobStagePlan, MetadataMap};
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
    ) -> Result<(), ApiError> {
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
        ) -> Result<(), ApiError> {
            Ok(())
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

/// Runner that tracks how many instances of itself are executing
/// concurrently (peak observed), then sleeps briefly before completing —
/// long enough that overlapping claims would show up as concurrency > 1 if
/// the crawl-specific gate were not enforced.
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
    ) -> Result<(), ApiError> {
        use std::sync::atomic::Ordering;
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        self.current.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Regression test for fix 3: crawl jobs must stay bounded by
/// `crawl_job_concurrency_limit` even when the general
/// `unified_worker_concurrency` semaphore is set much higher — crawl jobs
/// share exactly one Chrome instance, so letting them freely consume general
/// worker slots (as every other job kind does) risks CDP session contention.
#[tokio::test]
async fn crawl_jobs_stay_bounded_by_crawl_specific_limit_even_with_high_general_concurrency() {
    let (pool, _temp) = test_pool().await;
    let pool = Arc::new(pool);

    const CRAWL_JOBS: usize = 4;
    for _ in 0..CRAWL_JOBS {
        enqueue_test_job(&pool, UnifiedJobKind::Crawl).await;
    }

    let current = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        UnifiedJobKind::Crawl,
        Arc::new(ConcurrencyTrackingRunner {
            current: Arc::clone(&current),
            peak: Arc::clone(&peak),
        }),
    );
    let registry = Arc::new(registry);

    let notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();

    // High general concurrency (8), but crawl-specific limit of 1 — matching
    // Config::crawl_job_concurrency_limit's default.
    let handle = tokio::spawn(unified_worker_loop_with_concurrency_limits(
        Arc::clone(&pool),
        Arc::clone(&notify),
        shutdown.clone(),
        Some(registry),
        8,
        1,
    ));

    // Poll until every crawl job has reached a terminal state (bounded so a
    // regression hangs the test instead of looping forever). Re-notify on
    // every poll tick rather than relying on a single notify_one() racing the
    // worker task's startup (ensure_no_incompatible_legacy_jobs + first
    // select! registration) — the fallback POLL_INTERVAL is 5s, so a lost
    // single notify would otherwise make this test flaky under load rather
    // than a real regression.
    let store = SqliteUnifiedJobStore::new((*pool).clone());
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            notify.notify_one();
            let page = store
                .list(axon_api::source::JobListRequest {
                    kind: Some(UnifiedJobKind::Crawl),
                    status: None,
                    source_id: None,
                    watch_id: None,
                    limit: Some(CRAWL_JOBS as u32),
                    cursor: None,
                })
                .await
                .unwrap();
            if page.items.len() == CRAWL_JOBS
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
    .expect("all crawl jobs should complete within 20s");

    shutdown.cancel();
    let _ = handle.await;

    assert_eq!(
        peak.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "at most one crawl job should ever run concurrently despite concurrency=8"
    );
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

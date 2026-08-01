use super::*;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

use async_trait::async_trait;
use axon_api::source::JobKind;
use axon_core::config::Config;
use axon_jobs::status::JobStatus;

use crate::runtime::{RuntimeResult, ServiceJobRuntime};
use crate::test_support::NoopServiceRuntime;

/// Fake runtime whose queue reads active for the first `active_polls`
/// `has_active_jobs` calls, then idle. Counts recover sweeps.
///
/// Only `has_active_jobs` and `recover_jobs` carry test behaviour — every other
/// method delegates to the crate's canonical [`NoopServiceRuntime`] so this fake
/// never re-implements (and can never silently drift from) the full
/// `ServiceJobRuntime` stub surface (`axon_rust-x4gxr.12`).
struct FakeQueueRuntime {
    inner: NoopServiceRuntime,
    active_polls: AtomicI64,
    recover_calls: AtomicU64,
    in_flight: AtomicUsize,
}

impl FakeQueueRuntime {
    fn with_active_polls(active_polls: i64) -> Self {
        Self {
            inner: NoopServiceRuntime,
            active_polls: AtomicI64::new(active_polls),
            recover_calls: AtomicU64::new(0),
            in_flight: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl ServiceJobRuntime for FakeQueueRuntime {
    fn mode_name(&self) -> &'static str {
        self.inner.mode_name()
    }

    fn worker_in_flight_jobs(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }

    async fn wait_for_job(&self, id: uuid::Uuid, kind: JobKind) -> RuntimeResult<String> {
        self.inner.wait_for_job(id, kind).await
    }

    async fn job_errors(&self, id: uuid::Uuid, kind: JobKind) -> RuntimeResult<Option<String>> {
        self.inner.job_errors(id, kind).await
    }

    async fn has_active_jobs(&self, kind: JobKind) -> RuntimeResult<bool> {
        // Only decrement on the first watched kind so one loop iteration
        // consumes exactly one poll credit.
        if kind != WORKER_JOB_KINDS[0] {
            return Ok(false);
        }
        Ok(self.active_polls.fetch_sub(1, Ordering::SeqCst) > 0)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> RuntimeResult<Vec<crate::types::ServiceJob>> {
        self.inner.list_jobs(kind, limit, offset).await
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: uuid::Uuid,
    ) -> RuntimeResult<Option<crate::types::ServiceJob>> {
        self.inner.job_status(kind, id).await
    }

    async fn cancel_job(&self, kind: JobKind, id: uuid::Uuid) -> RuntimeResult<bool> {
        self.inner.cancel_job(kind, id).await
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> RuntimeResult<u64> {
        self.inner.cleanup_jobs(kind).await
    }

    async fn clear_jobs(&self, kind: JobKind) -> RuntimeResult<u64> {
        self.inner.clear_jobs(kind).await
    }

    async fn recover_jobs(&self, _kind: JobKind, _stale_threshold_ms: i64) -> RuntimeResult<u64> {
        self.recover_calls.fetch_add(1, Ordering::SeqCst);
        Ok(1)
    }

    async fn count_jobs(&self, kind: JobKind) -> RuntimeResult<i64> {
        self.inner.count_jobs(kind).await
    }

    async fn count_jobs_by_status(&self, kind: JobKind) -> RuntimeResult<HashMap<JobStatus, i64>> {
        self.inner.count_jobs_by_status(kind).await
    }
}

fn context_with(runtime: Arc<FakeQueueRuntime>) -> crate::context::ServiceContext {
    crate::context::ServiceContext::from_runtime(Arc::new(Config::test_default()), runtime)
}

#[tokio::test(start_paused = true)]
async fn exits_after_continuous_idle_window() {
    let runtime = Arc::new(FakeQueueRuntime::with_active_polls(2));
    let ctx = context_with(Arc::clone(&runtime));

    let report = run_worker_until_idle(&ctx, WorkerLoopOptions { idle_exit_secs: 3 })
        .await
        .expect("worker loop");

    // 2 active polls + 3 idle seconds — with paused time the loop advances
    // virtually, so this stays instant in wall-clock terms.
    assert!(report.elapsed_secs >= 4, "elapsed={}", report.elapsed_secs);
    // Startup sweep runs once across both watched kinds.
    assert_eq!(report.recovered_jobs, WORKER_JOB_KINDS.len() as u64);
    assert!(runtime.recover_calls.load(Ordering::SeqCst) >= 2);
}

#[tokio::test(start_paused = true)]
async fn idle_queue_exits_after_exactly_the_idle_window() {
    let runtime = Arc::new(FakeQueueRuntime::with_active_polls(0));
    let ctx = context_with(Arc::clone(&runtime));

    let report = run_worker_until_idle(&ctx, WorkerLoopOptions { idle_exit_secs: 2 })
        .await
        .expect("worker loop");

    assert!(report.elapsed_secs >= 2, "elapsed={}", report.elapsed_secs);
    assert!(report.elapsed_secs <= 5, "elapsed={}", report.elapsed_secs);
}

#[tokio::test(start_paused = true)]
async fn does_not_idle_exit_while_process_local_runner_is_in_flight() {
    let runtime = Arc::new(FakeQueueRuntime::with_active_polls(0));
    runtime.in_flight.store(1, Ordering::Release);
    let ctx = context_with(Arc::clone(&runtime));

    let loop_task = tokio::spawn(async move {
        run_worker_until_idle(&ctx, WorkerLoopOptions { idle_exit_secs: 2 })
            .await
            .expect("worker loop")
    });

    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        !loop_task.is_finished(),
        "idle exit must not trust an empty durable projection while a runner is alive"
    );

    runtime.in_flight.store(0, Ordering::Release);
    let report = loop_task.await.expect("worker task");
    assert!(report.elapsed_secs >= 7, "elapsed={}", report.elapsed_secs);
}

#[tokio::test]
async fn durable_running_source_row_prevents_idle_exit() {
    let sqlite = crate::test_support::sqlite_test_runtime()
        .await
        .expect("sqlite runtime");
    let pool = sqlite.runtime.sqlite_pool().expect("sqlite pool");
    let now = chrono::Utc::now().to_rfc3339();
    let job_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO jobs (
            job_id, kind, intent, status, phase, priority, attempt, created_at, updated_at
         ) VALUES (?, 'source', 'run', 'running', 'fetching', 'normal', 1, ?, ?)",
    )
    .bind(&job_id)
    .bind(&now)
    .bind(&now)
    .execute(pool.as_ref())
    .await
    .expect("insert running source");

    let ctx = crate::context::ServiceContext::from_runtime(
        Arc::new(Config::test_default()),
        Arc::clone(&sqlite.runtime),
    );
    let loop_task = tokio::spawn(async move {
        run_worker_until_idle(&ctx, WorkerLoopOptions { idle_exit_secs: 2 })
            .await
            .expect("worker loop")
    });

    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !loop_task.is_finished(),
        "a real durable running Source row must hold the standalone worker open"
    );

    sqlx::query(
        "UPDATE jobs SET status = 'completed', phase = 'complete', finished_at = ? \
         WHERE job_id = ?",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(job_id)
    .execute(pool.as_ref())
    .await
    .expect("terminalize source");
    let report = tokio::time::timeout(Duration::from_secs(5), loop_task)
        .await
        .expect("worker should exit after the row becomes terminal")
        .expect("worker task");
    assert!(!report.final_active_jobs);
    assert_eq!(report.final_in_flight_jobs, 0);
}

pub mod auth_enforcement;
mod cancel_registry;
mod spawn_unified;
mod starvation;
pub mod unified;
mod watch_scheduler;
mod watchdog;

use spawn_unified::spawn_unified_worker;
pub use unified::{JobRunnerRegistry, UnifiedJobOutcome, UnifiedJobRunner};

pub(crate) use cancel_registry::cancel_job;

use axon_core::config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Shared with the unified worker loop (`workers/unified.rs`): poll fallback
/// interval when `notify_unified()` is not fired, and the per-wake claim batch
/// cap.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;

#[derive(Debug, Default)]
pub(crate) struct WorkerActivity {
    in_flight: AtomicUsize,
}

impl WorkerActivity {
    fn begin(self: &Arc<Self>) -> WorkerActivityGuard {
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        WorkerActivityGuard(Arc::clone(self))
    }

    fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }
}

struct WorkerActivityGuard(Arc<WorkerActivity>);

impl Drop for WorkerActivityGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Handles to wake unified worker tasks.
pub struct WorkerHandles {
    pub(crate) unified: Arc<Notify>,
    activity: Arc<WorkerActivity>,
    shutdown: CancellationToken,
    pub(crate) worker_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WorkerHandles {
    /// Notify the unified durable-job worker that a job-backed operation was queued.
    pub fn notify_unified(&self) {
        self.unified.notify_one();
    }

    /// Number of jobs claimed by this process whose runner has not returned.
    ///
    /// This is the authoritative process-local drain signal. Durable rows are
    /// still queried for queued work, but an idle-exit decision must not rely
    /// on a database projection alone while a claimed task remains alive.
    pub fn in_flight_jobs(&self) -> usize {
        self.activity.in_flight()
    }

    /// Cancel worker admission and wait for every worker task to finish.
    /// Non-cooperative tasks are explicitly aborted once the shared deadline
    /// expires, so dropping a join handle can never silently detach work.
    pub async fn shutdown_and_join(mut self, grace: Duration) {
        self.shutdown.cancel();
        self.unified.notify_waiters();
        let deadline = tokio::time::Instant::now() + grace;
        for mut handle in std::mem::take(&mut self.worker_handles) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if tokio::time::timeout(remaining, &mut handle).await.is_err() {
                handle.abort();
                let _ = handle.await;
            }
        }
    }
}

#[cfg(test)]
#[path = "workers_tests.rs"]
mod tests;

impl Drop for WorkerHandles {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.unified.notify_waiters();
    }
}

/// Spawn in-process worker tasks for unified jobs and recurring watches.
pub fn spawn_workers(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    job_runner_registry: Option<Arc<JobRunnerRegistry>>,
) -> WorkerHandles {
    let unified_notify = Arc::new(Notify::new());
    let activity = Arc::new(WorkerActivity::default());
    let shutdown = CancellationToken::new();

    tracing::info!(
        unified_worker_concurrency = cfg.unified_worker_concurrency,
        "jobs: spawning in-process unified workers"
    );

    let worker_handles = vec![
        spawn_unified_worker(
            Arc::clone(&pool),
            Arc::clone(&unified_notify),
            Arc::clone(&activity),
            shutdown.clone(),
            job_runner_registry,
            cfg.unified_worker_concurrency,
            cfg.source_job_concurrency_limit,
        ),
        tokio::spawn(watchdog::watchdog_loop(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            WatchdogNotifies {
                unified: Arc::clone(&unified_notify),
            },
            shutdown.clone(),
        )),
        tokio::spawn(watch_scheduler::watch_scheduler_loop(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&unified_notify),
            shutdown.clone(),
        )),
    ];

    WorkerHandles {
        unified: unified_notify,
        activity,
        shutdown,
        worker_handles,
    }
}

struct WatchdogNotifies {
    unified: Arc<Notify>,
}

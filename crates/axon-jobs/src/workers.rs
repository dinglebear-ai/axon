pub mod auth_enforcement;
mod spawn_unified;
mod starvation;
pub mod unified;
mod watch_scheduler;
mod watchdog;

use spawn_unified::spawn_unified_worker;
pub use unified::{JobRunnerRegistry, UnifiedJobRunner};

use axon_core::config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Shared with the unified worker loop (`workers/unified.rs`): poll fallback
/// interval when `notify_unified()` is not fired, and the per-wake claim batch
/// cap.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;

/// Handles to wake unified worker tasks.
pub struct WorkerHandles {
    pub(crate) unified: Arc<Notify>,
    shutdown: CancellationToken,
    #[allow(dead_code)]
    pub(crate) worker_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WorkerHandles {
    /// Notify the unified durable-job worker that a job-backed operation was queued.
    pub fn notify_unified(&self) {
        self.unified.notify_one();
    }
}

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
    let shutdown = CancellationToken::new();

    tracing::info!(
        unified_worker_concurrency = cfg.unified_worker_concurrency,
        "jobs: spawning in-process unified workers"
    );

    let worker_handles = vec![
        spawn_unified_worker(
            Arc::clone(&pool),
            Arc::clone(&unified_notify),
            shutdown.clone(),
            job_runner_registry,
            cfg.unified_worker_concurrency,
            cfg.crawl_job_concurrency_limit,
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
        shutdown,
        worker_handles,
    }
}

struct WatchdogNotifies {
    unified: Arc<Notify>,
}

pub mod auth_enforcement;
mod spawn_unified;
mod starvation;
pub mod unified;
mod watch_scheduler;
mod watchdog;

use spawn_unified::spawn_unified_worker;
pub use unified::{JobRunnerRegistry, UnifiedJobRunner};

use crate::backend::JobKind;

use crate::cancel::CancelStore;
use axon_core::config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Shared with the unified worker loop (`workers/unified.rs`): poll fallback
/// interval when `notify_unified()` isn't fired, and the per-wake claim batch
/// cap.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;

/// Handles to wake specific worker types when new jobs are enqueued.
pub struct WorkerHandles {
    pub(crate) crawl: Arc<Notify>,
    pub(crate) embed: Arc<Notify>,
    pub(crate) extract: Arc<Notify>,
    pub(crate) ingest: Arc<Notify>,
    pub(crate) unified: Arc<Notify>,
    shutdown: CancellationToken,
    /// Actual worker loops. Dropping WorkerHandles requests graceful shutdown;
    /// tasks observe it before polling and between jobs/batches.
    #[allow(dead_code)]
    pub(crate) worker_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WorkerHandles {
    /// Notify the worker for the given job kind that a new job is available.
    pub(crate) fn notify(&self, kind: JobKind) {
        match kind {
            JobKind::Crawl => self.crawl.notify_one(),
            JobKind::Embed => self.embed.notify_one(),
            JobKind::Extract => self.extract.notify_one(),
            JobKind::Ingest => self.ingest.notify_one(),
        }
    }

    /// Notify the unified durable-job worker that a job-backed operation was queued.
    pub fn notify_unified(&self) {
        self.unified.notify_one();
    }
}

impl Drop for WorkerHandles {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.crawl.notify_waiters();
        self.embed.notify_waiters();
        self.extract.notify_waiters();
        self.ingest.notify_waiters();
        self.unified.notify_waiters();
    }
}

/// Spawn in-process worker tasks for all job types.
///
/// Crawl, Embed, and Ingest no longer run legacy per-family lanes here — they
/// execute on the unified worker (see the comment below). `cfg.embed_lanes`/
/// `cfg.ingest_lanes` are dead config kept only for the migration window; a
/// later cleanup pass should remove them.
pub fn spawn_workers(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    job_runner_registry: Option<Arc<JobRunnerRegistry>>,
) -> WorkerHandles {
    let crawl_notify = Arc::new(Notify::new());
    let embed_notify = Arc::new(Notify::new());
    let extract_notify = Arc::new(Notify::new());
    let ingest_notify = Arc::new(Notify::new());
    let unified_notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();

    let mut worker_handles = Vec::new();

    tracing::info!(
        unified_worker_concurrency = cfg.unified_worker_concurrency,
        "jobs: spawning in-process job workers"
    );

    worker_handles.push(spawn_unified_worker(
        Arc::clone(&pool),
        Arc::clone(&unified_notify),
        shutdown.clone(),
        job_runner_registry,
        cfg.unified_worker_concurrency,
        cfg.crawl_job_concurrency_limit,
    ));

    // Crawl, Embed, and Ingest no longer have legacy in-process worker lanes —
    // real execution for `JobKind::{Crawl,Embed,Ingest}` runs on the unified
    // worker (`crates/axon-services/src/runtime/job_runners/*`), matching the
    // Extract cutover. `crawl_notify`/`embed_notify`/`ingest_notify` stay
    // wired into the watchdog's generic reclaim sweep below so any
    // pre-cutover rows still in `axon_crawl_jobs`/`axon_embed_jobs`/
    // `axon_ingest_jobs` are still reclaimable to a terminal state (no new
    // legacy-row execution — reclaim-to-failed only).

    // Periodic watchdog: sweeps all job tables every 15s; wakes workers on reclaim.
    worker_handles.push(tokio::spawn(watchdog::watchdog_loop(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        WatchdogNotifies {
            crawl: Arc::clone(&crawl_notify),
            embed: Arc::clone(&embed_notify),
            extract: Arc::clone(&extract_notify),
            ingest: Arc::clone(&ingest_notify),
            unified: Arc::clone(&unified_notify),
        },
        shutdown.clone(),
    )));

    // Watch scheduler: fires recurring watch definitions whose next_run_at has
    // passed. Self-contained — leases its own due rows, no Notify channel.
    tracing::info!(worker = "watch_scheduler", "jobs: spawning worker");
    worker_handles.push(tokio::spawn(watch_scheduler::watch_scheduler_loop(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        shutdown.clone(),
    )));

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        unified: unified_notify,
        shutdown,
        worker_handles,
    }
}

/// Per-kind `Notify` handles the watchdog uses to wake parked worker lanes
/// after reclaiming stale jobs or detecting a starved queue. The loop itself
/// lives in the `watchdog` submodule.
struct WatchdogNotifies {
    crawl: Arc<Notify>,
    embed: Arc<Notify>,
    extract: Arc<Notify>,
    ingest: Arc<Notify>,
    /// Wakes the unified durable-job worker lane after the watchdog reclaims
    /// stale `running` rows in the unified `jobs` table (see
    /// `watchdog::watchdog_loop`'s unified-store reclaim sweep).
    unified: Arc<Notify>,
}

impl WatchdogNotifies {
    /// Wake every parked lane of `kind` (`notify_waiters`, matching the reclaim
    /// path). Used by the starvation detector to kick a stalled-but-alive lane.
    fn notify_kind(&self, kind: JobKind) {
        match kind {
            JobKind::Crawl => self.crawl.notify_waiters(),
            JobKind::Embed => self.embed.notify_waiters(),
            JobKind::Extract => self.extract.notify_waiters(),
            JobKind::Ingest => self.ingest.notify_waiters(),
        }
    }
}

#[cfg(test)]
#[path = "workers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "security_error_memory_e2e_tests.rs"]
mod security_error_memory_e2e_tests;

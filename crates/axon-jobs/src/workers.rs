mod heartbeat;
mod panic_guard;
mod progress;
mod runners;
mod starvation;
mod watch_scheduler;
mod watchdog;

use heartbeat::HeartbeatGuard;

use runners::{JobResult, run_crawl_job, run_embed_job, run_extract_job, run_ingest_job};

use crate::backend::JobKind;

use crate::cancel::CancelStore;
use crate::ops::{
    ClaimedJob, claim_next_pending_for_attempt, mark_completed_for_attempt, mark_failed_for_attempt,
};
use axon_core::config::Config;
use sqlx::SqlitePool;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;

/// Handles to wake specific worker types when new jobs are enqueued.
pub struct WorkerHandles {
    pub(crate) crawl: Arc<Notify>,
    pub(crate) embed: Arc<Notify>,
    pub(crate) extract: Arc<Notify>,
    pub(crate) ingest: Arc<Notify>,
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
}

impl Drop for WorkerHandles {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.crawl.notify_waiters();
        self.embed.notify_waiters();
        self.extract.notify_waiters();
        self.ingest.notify_waiters();
    }
}

/// Spawn in-process worker tasks for all job types.
///
/// Embed and ingest spawn multiple lanes from `Config`. All lanes share the
/// same `Notify` handle so a single `notify_one()` wakes one waiting lane.
/// SQLite `BEGIN IMMEDIATE` in `claim_next_pending()` serializes lane claims
/// atomically — no semaphore needed.
pub fn spawn_workers(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
) -> WorkerHandles {
    let crawl_notify = Arc::new(Notify::new());
    let embed_notify = Arc::new(Notify::new());
    let extract_notify = Arc::new(Notify::new());
    let ingest_notify = Arc::new(Notify::new());
    let shutdown = CancellationToken::new();

    let embed_lanes = cfg.embed_lanes.clamp(1, 32);
    // ingest_lanes is sourced from Config (env > TOML > default), already clamped at parse
    // time to the same effective range used here.
    let ingest_lanes = cfg.ingest_lanes.clamp(1, 16);

    let mut worker_handles = Vec::new();

    tracing::info!(
        embed_lanes,
        ingest_lanes,
        "jobs: spawning in-process job workers"
    );

    // Crawl: single lane (spider futures are !Send — must stay single-task)
    tracing::info!(worker = "crawl", lanes = 1, "jobs: spawning worker");
    worker_handles.push(tokio::spawn(crawl_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        Arc::clone(&crawl_notify),
        Arc::clone(&embed_notify),
        shutdown.clone(),
    )));

    // Embed: multi-lane
    tracing::info!(
        worker = "embed",
        lanes = embed_lanes,
        "jobs: spawning workers"
    );
    for lane in 0..embed_lanes {
        tracing::debug!(worker = "embed", lane, "jobs: spawning embed lane");
        worker_handles.push(tokio::spawn(embed_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
            Arc::clone(&embed_notify),
            shutdown.clone(),
        )));
    }

    // Extract: single lane
    tracing::info!(worker = "extract", lanes = 1, "jobs: spawning worker");
    worker_handles.push(tokio::spawn(extract_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        Arc::clone(&extract_notify),
        shutdown.clone(),
    )));

    // Ingest: multi-lane
    tracing::info!(
        worker = "ingest",
        lanes = ingest_lanes,
        "jobs: spawning workers"
    );
    for lane in 0..ingest_lanes {
        tracing::debug!(worker = "ingest", lane, "jobs: spawning ingest lane");
        worker_handles.push(tokio::spawn(ingest_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
            Arc::clone(&ingest_notify),
            shutdown.clone(),
        )));
    }

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

/// Generic worker loop: wait for Notify or poll timeout, then claim + run pending jobs.
///
/// Jobs are processed in bounded batches so multi-lane worker sets can yield
/// between bursts and observe shutdown. Shutdown is graceful between jobs; an
/// already-running job is allowed to finish and mark its terminal state. If the
/// process dies mid-job, stale-job recovery reclaims the running row on restart.
async fn worker_loop<F, Fut>(
    pool: Arc<SqlitePool>,
    kind: JobKind,
    cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
    run_job: F,
) where
    F: Fn(Arc<SqlitePool>, uuid::Uuid, CancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = JobResult> + Send,
{
    let mut wake_count: u64 = 0;
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = shutdown.cancelled() => break,
        }
        wake_count = wake_count.wrapping_add(1);
        tracing::trace!(
            table = kind.table_name(),
            wake_count,
            "worker: wake — polling for pending jobs"
        );

        let mut claimed_this_wake = 0usize;
        loop {
            let mut processed = 0usize;
            while processed < WORKER_BATCH_LIMIT && !shutdown.is_cancelled() {
                match claim_next_pending_for_attempt(&pool, kind).await {
                    Ok(Some(claimed)) => {
                        run_and_mark_claimed(&pool, kind, &cancel_store, &claimed, &run_job).await;
                        processed += 1;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        // write contention during job claim is normal under concurrent workers
                        let is_busy = crate::ops::is_lock_busy(&e);
                        if is_busy {
                            tracing::warn!(
                                table = kind.table_name(),
                                error = %e,
                                "worker claim skipped — DB busy; will retry"
                            );
                        } else {
                            crate::store::record_sqlite_runtime_error(&e);
                            tracing::error!(
                                table = kind.table_name(),
                                error = %e,
                                "worker claim error"
                            );
                        }
                        break;
                    }
                }
            }
            claimed_this_wake += processed;
            if shutdown.is_cancelled() || processed < WORKER_BATCH_LIMIT {
                break;
            }
            tokio::task::yield_now().await;
        }
        // Cheap liveness signal: log when work was done, and roughly once a
        // minute (every 12th 5s wake) while idle, so a lane that has gone silent
        // is visible by the *absence* of these lines.
        if claimed_this_wake > 0 || wake_count.is_multiple_of(12) {
            tracing::debug!(
                table = kind.table_name(),
                claimed = claimed_this_wake,
                wake_count,
                "worker: poll batch complete"
            );
        }
    }
}

/// Run one claimed job through the panic guard and persist its terminal state.
/// Extracted from `worker_loop` to keep that function under the size cap; a
/// panic in the runner is caught here (see `panic_guard::run_catching`) so the
/// lane survives, and a failure to write the terminal state is logged (the row
/// stays `running` and is later reclaimed by the watchdog).
async fn run_and_mark_claimed<F, Fut>(
    pool: &Arc<SqlitePool>,
    kind: JobKind,
    cancel_store: &CancelStore,
    claimed: &ClaimedJob,
    run_job: &F,
) where
    F: Fn(Arc<SqlitePool>, uuid::Uuid, CancellationToken) -> Fut,
    Fut: Future<Output = JobResult>,
{
    let cancel_token = cancel_store.register(claimed.id, claimed.attempt_id.clone());
    let _hb = HeartbeatGuard::spawn(
        Arc::clone(pool),
        kind,
        claimed.id,
        claimed.attempt_id.clone(),
    );
    let result = panic_guard::run_catching(
        run_job(Arc::clone(pool), claimed.id, cancel_token),
        kind,
        claimed.id,
    )
    .await;
    cancel_store.remove(claimed.id, &claimed.attempt_id);
    match result {
        Ok(result_json) => {
            if let Err(e) = mark_completed_for_attempt(
                pool,
                kind,
                claimed.id,
                Some(&claimed.attempt_id),
                result_json.as_ref(),
            )
            .await
            {
                tracing::error!(
                    table = kind.table_name(),
                    job_id = %claimed.id,
                    error = %e,
                    "job worker: failed to mark job completed — job will remain in 'running' state"
                );
            }
        }
        Err(e) => {
            if let Err(mark_err) = mark_failed_for_attempt(
                pool,
                kind,
                claimed.id,
                Some(&claimed.attempt_id),
                &e.to_string(),
            )
            .await
            {
                tracing::error!(
                    table = kind.table_name(),
                    job_id = %claimed.id,
                    error = %mark_err,
                    "job worker: failed to mark job failed — job will remain in 'running' state"
                );
            }
        }
    }
}

async fn crawl_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
    embed_notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    worker_loop(
        pool,
        JobKind::Crawl,
        cancel_store,
        notify,
        shutdown,
        move |pool, id, token| {
            let cfg = Arc::clone(&cfg);
            let embed_notify = Arc::clone(&embed_notify);
            async move { run_crawl_job(&pool, &cfg, id, Some(embed_notify), Some(token)).await }
        },
    )
    .await;
}

async fn embed_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    worker_loop(
        pool,
        JobKind::Embed,
        cancel_store,
        notify,
        shutdown,
        move |pool, id, token| {
            let cfg = Arc::clone(&cfg);
            async move { run_embed_job(&pool, &cfg, id, Some(token)).await }
        },
    )
    .await;
}

async fn extract_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    worker_loop(
        pool,
        JobKind::Extract,
        cancel_store,
        notify,
        shutdown,
        move |pool, id, token| {
            let cfg = Arc::clone(&cfg);
            async move { run_extract_job(&pool, &cfg, id, Some(token)).await }
        },
    )
    .await;
}

async fn ingest_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    worker_loop(
        pool,
        JobKind::Ingest,
        cancel_store,
        notify,
        shutdown,
        move |pool, id, token| {
            let cfg = Arc::clone(&cfg);
            async move { run_ingest_job(&pool, &cfg, id, Some(token)).await }
        },
    )
    .await;
}

#[cfg(test)]
#[path = "workers_tests.rs"]
mod tests;

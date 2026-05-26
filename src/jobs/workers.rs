mod heartbeat;
mod progress;
mod runners;

use heartbeat::HeartbeatGuard;

use runners::{JobResult, run_crawl_job, run_embed_job, run_extract_job, run_ingest_job};

use crate::jobs::backend::JobKind;

use crate::core::config::Config;
use crate::jobs::cancel::CancelStore;
use crate::jobs::ops::{
    claim_next_pending_for_attempt, mark_completed_for_attempt, mark_failed_for_attempt,
};
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
    worker_handles.push(tokio::spawn(watchdog_loop(
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

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        shutdown,
        worker_handles,
    }
}

/// Periodic watchdog: sweeps all job tables on a config-driven interval
/// (`cfg.watchdog_sweep_secs`, default 15s) while the process is alive.
///
/// Pairs with `HeartbeatGuard` — heartbeat keeps `updated_at` fresh for live jobs;
/// watchdog reclaims rows whose `updated_at` has gone stale (process died, runner
/// panicked, etc.). After a reclaim, wakes the worker channels whose kind had
/// rows reclaimed — not the others — so untouched lanes stay parked.
struct WatchdogNotifies {
    crawl: Arc<Notify>,
    embed: Arc<Notify>,
    extract: Arc<Notify>,
    ingest: Arc<Notify>,
}

async fn watchdog_loop(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
    notifies: WatchdogNotifies,
    shutdown: CancellationToken,
) {
    let stale_threshold_ms =
        (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000i64;
    let sweep_interval = Duration::from_secs(cfg.watchdog_sweep_secs.max(1) as u64);
    let mut ticker = tokio::time::interval(sweep_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip immediate tick — startup-time reclaim already ran in SqliteJobBackend::init.
    ticker.tick().await;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                match crate::jobs::store::reclaim_stale_running_jobs_detailed(
                    &pool,
                    stale_threshold_ms,
                )
                .await
                {
                    Ok(reclaimed) if reclaimed.total() > 0 => {
                        cancel_reclaimed_local_tokens(&cancel_store, &reclaimed);
                        // notify_waiters (not notify_one) so all parked lanes
                        // for each kind wake — a single reclaim sweep can free
                        // multiple jobs of the same kind, and embed/ingest run
                        // multiple lanes that share one Notify handle.
                        if reclaimed.count_for(JobKind::Crawl) > 0 {
                            notifies.crawl.notify_waiters();
                        }
                        if reclaimed.count_for(JobKind::Embed) > 0 {
                            notifies.embed.notify_waiters();
                        }
                        if reclaimed.count_for(JobKind::Extract) > 0 {
                            notifies.extract.notify_waiters();
                        }
                        if reclaimed.count_for(JobKind::Ingest) > 0 {
                            notifies.ingest.notify_waiters();
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "watchdog: periodic reclaim failed");
                    }
                }
            }
        }
    }
}

fn cancel_reclaimed_local_tokens(
    cancel_store: &CancelStore,
    reclaimed: &crate::jobs::store::ReclaimedJobs,
) {
    for kind in JobKind::all() {
        for reclaimed_job in reclaimed.jobs_for(*kind) {
            let canceled = reclaimed_job
                .attempt_id
                .as_deref()
                .is_some_and(|attempt_id| cancel_store.cancel_local(reclaimed_job.id, attempt_id));
            tracing::info!(
                table = kind.table_name(),
                job_id = %reclaimed_job.id,
                attempt_id = reclaimed_job.attempt_id.as_deref().unwrap_or("unknown"),
                canceled_local_token = canceled,
                "watchdog: canceled local owner for reclaimed job"
            );
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
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = shutdown.cancelled() => break,
        }

        loop {
            let mut processed = 0usize;
            while processed < WORKER_BATCH_LIMIT && !shutdown.is_cancelled() {
                match claim_next_pending_for_attempt(&pool, kind).await {
                    Ok(Some(claimed)) => {
                        let cancel_token =
                            cancel_store.register(claimed.id, claimed.attempt_id.clone());
                        let _hb = HeartbeatGuard::spawn(
                            Arc::clone(&pool),
                            kind,
                            claimed.id,
                            claimed.attempt_id.clone(),
                        );
                        let result = run_job(Arc::clone(&pool), claimed.id, cancel_token).await;
                        cancel_store.remove(claimed.id, &claimed.attempt_id);
                        processed += 1;
                        match result {
                            Ok(result_json) => {
                                if let Err(e) = mark_completed_for_attempt(
                                    &pool,
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
                                    &pool,
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
                    Ok(None) => break,
                    Err(e) => {
                        // write contention during job claim is normal under concurrent workers
                        let is_busy = crate::jobs::ops::is_lock_busy(&e);
                        if is_busy {
                            tracing::warn!(
                                table = kind.table_name(),
                                error = %e,
                                "worker claim skipped — DB busy; will retry"
                            );
                        } else {
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
            if shutdown.is_cancelled() || processed < WORKER_BATCH_LIMIT {
                break;
            }
            tokio::task::yield_now().await;
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

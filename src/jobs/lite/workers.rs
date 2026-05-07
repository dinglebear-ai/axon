mod heartbeat;
mod progress;
mod runners;

use heartbeat::HeartbeatGuard;

use runners::{
    JobResult, run_crawl_job_lite, run_embed_job_lite, run_extract_job_lite, run_ingest_job_lite,
};

use crate::jobs::backend::JobKind;

use crate::core::config::Config;
use crate::jobs::lite::cancel::CancelStore;
use crate::jobs::lite::ops::{claim_next_pending, mark_completed, mark_failed};
use sqlx::SqlitePool;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Resolve the number of worker lanes for a job type from an env var.
///
/// Falls back to a CPU-scaled default clamped to `[cpu_min, cpu_max]`.
pub(crate) fn resolve_lane_count(env_var: &str, cpu_min: usize, cpu_max: usize) -> usize {
    let cpu_default = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(cpu_min, cpu_max);
    std::env::var(env_var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(cpu_default)
}

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

/// Spawn in-process worker tasks for all 6 job types.
///
/// Embed and ingest spawn multiple lanes (controlled by `AXON_EMBED_LANES` /
/// `AXON_INGEST_LANES`). All lanes share the same `Notify` handle so a single
/// `notify_one()` wakes one waiting lane. SQLite `BEGIN IMMEDIATE` in
/// `claim_next_pending()` serializes lane claims atomically — no semaphore needed.
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

    let embed_lanes = resolve_lane_count("AXON_EMBED_LANES", 2, 32);
    let ingest_lanes = resolve_lane_count("AXON_INGEST_LANES", 2, 16);

    let mut worker_handles = Vec::new();

    tracing::info!(
        embed_lanes,
        ingest_lanes,
        "lite: spawning in-process job workers"
    );

    // Crawl: single lane (spider futures are !Send — must stay single-task)
    tracing::info!(worker = "crawl", lanes = 1, "lite: spawning worker");
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
        "lite: spawning workers"
    );
    for lane in 0..embed_lanes {
        tracing::debug!(worker = "embed", lane, "lite: spawning embed lane");
        worker_handles.push(tokio::spawn(embed_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
            Arc::clone(&embed_notify),
            shutdown.clone(),
        )));
    }

    // Extract: single lane
    tracing::info!(worker = "extract", lanes = 1, "lite: spawning worker");
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
        "lite: spawning workers"
    );
    for lane in 0..ingest_lanes {
        tracing::debug!(worker = "ingest", lane, "lite: spawning ingest lane");
        worker_handles.push(tokio::spawn(ingest_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&cancel_store),
            Arc::clone(&ingest_notify),
            shutdown.clone(),
        )));
    }

    // Periodic watchdog: re-runs reclaim_stale_running_jobs every 60s while the
    // process is alive. Pairs with the per-job HeartbeatGuard — heartbeat keeps
    // updated_at fresh for live jobs; watchdog reclaims rows whose updated_at
    // has gone stale (process died, runner panicked, etc.).
    {
        let pool = Arc::clone(&pool);
        let cfg_for_watchdog = Arc::clone(&cfg);
        let shutdown = shutdown.clone();
        worker_handles.push(tokio::spawn(async move {
            let stale_threshold_ms = (cfg_for_watchdog.watchdog_stale_timeout_secs
                + cfg_for_watchdog.watchdog_confirm_secs)
                .max(0)
                * 1_000i64;
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // Skip immediate tick — startup-time reclaim already ran in LiteBackend::init.
            ticker.tick().await;
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = ticker.tick() => {
                        if let Err(e) = crate::jobs::lite::store::reclaim_stale_running_jobs(
                            &pool,
                            stale_threshold_ms,
                        )
                        .await
                        {
                            tracing::warn!(error = %e, "watchdog: periodic reclaim failed");
                        }
                    }
                }
            }
        }));
    }

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        shutdown,
        worker_handles,
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
                match claim_next_pending(&pool, kind).await {
                    Ok(Some(id)) => {
                        let cancel_token = cancel_store.register(id);
                        let _hb = HeartbeatGuard::spawn(Arc::clone(&pool), kind, id);
                        let result = run_job(Arc::clone(&pool), id, cancel_token).await;
                        cancel_store.remove(id);
                        processed += 1;
                        match result {
                            Ok(result_json) => {
                                if let Err(e) =
                                    mark_completed(&pool, kind, id, result_json.as_ref()).await
                                {
                                    tracing::error!(
                                        table = kind.table_name(),
                                        job_id = %id,
                                        error = %e,
                                        "lite worker: failed to mark job completed — job will remain in 'running' state"
                                    );
                                }
                            }
                            Err(e) => {
                                if let Err(mark_err) =
                                    mark_failed(&pool, kind, id, &e.to_string()).await
                                {
                                    tracing::error!(
                                        table = kind.table_name(),
                                        job_id = %id,
                                        error = %mark_err,
                                        "lite worker: failed to mark job failed — job will remain in 'running' state"
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!(
                            table = kind.table_name(),
                            error = %e,
                            "worker claim error"
                        );
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
            async move {
                run_crawl_job_lite(&pool, &cfg, id, Some(embed_notify), Some(token)).await
            }
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
            async move { run_embed_job_lite(&pool, &cfg, id, Some(token)).await }
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
            async move { run_extract_job_lite(&pool, &cfg, id, Some(token)).await }
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
            async move { run_ingest_job_lite(&pool, &cfg, id, Some(token)).await }
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::backend::JobPayload;
    use crate::jobs::lite::cancel::CancelStore;
    use crate::jobs::lite::ops::enqueue_job;
    use crate::jobs::lite::store::open_sqlite_pool;

    #[tokio::test]
    async fn worker_picks_up_job_via_notify() {
        let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
        let notify = Arc::new(Notify::new());

        let id = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "test content".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .unwrap();

        let pool2 = Arc::clone(&pool);
        let notify2 = Arc::clone(&notify);
        let (tx, rx) = tokio::sync::oneshot::channel::<uuid::Uuid>();
        tokio::spawn(async move {
            if let Some(claimed_id) = claim_next_pending(&pool2, JobKind::Embed).await.unwrap() {
                assert_eq!(claimed_id, id);
                notify2.notify_one();
                let _ = tx.send(claimed_id);
            }
        });

        notify.notify_one();
        let claimed = tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .expect("task did not complete within 5s")
            .expect("sender dropped without sending");
        assert_eq!(claimed, id);

        let row: (String,) = sqlx::query_as("SELECT status FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_ne!(row.0, "pending", "job should have been claimed");
    }

    #[tokio::test]
    async fn dropping_worker_handles_gracefully_stops_worker_loops() {
        let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
        let cfg = Arc::new(Config::default_lite());
        let cancel_store = Arc::new(CancelStore::new());

        let handles = spawn_workers(pool, cfg, cancel_store);
        let abort_handles: Vec<_> = handles
            .worker_handles
            .iter()
            .map(tokio::task::JoinHandle::abort_handle)
            .collect();

        drop(handles);

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if abort_handles
                    .iter()
                    .all(tokio::task::AbortHandle::is_finished)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker tasks should stop when WorkerHandles is dropped");
    }
}

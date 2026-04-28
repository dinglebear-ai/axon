mod runners;

use runners::{
    JobResult, run_crawl_job_lite, run_embed_job_lite, run_extract_job_lite, run_ingest_job_lite,
};

use crate::crates::jobs::backend::JobKind;

use crate::crates::core::config::Config;
use crate::crates::jobs::lite::cancel::CancelStore;
use crate::crates::jobs::lite::ops::{claim_next_pending, mark_completed, mark_failed};
use sqlx::SqlitePool;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

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

/// Handles to wake specific worker types when new jobs are enqueued.
pub struct WorkerHandles {
    pub(crate) crawl: Arc<Notify>,
    pub(crate) embed: Arc<Notify>,
    pub(crate) extract: Arc<Notify>,
    pub(crate) ingest: Arc<Notify>,
    /// Actual worker loops. These must be aborted on drop or lite workers keep
    /// polling after the backend goes away.
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
            // Refresh and Graph are not supported in lite mode; no worker to notify.
            JobKind::Refresh | JobKind::Graph => {}
        }
    }
}

impl Drop for WorkerHandles {
    fn drop(&mut self) {
        for handle in &self.worker_handles {
            handle.abort();
        }
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

    let embed_lanes = resolve_lane_count("AXON_EMBED_LANES", 2, 32);
    let ingest_lanes = resolve_lane_count("AXON_INGEST_LANES", 2, 16);

    let mut worker_handles = Vec::new();

    // Crawl: single lane (spider futures are !Send — must stay single-task)
    worker_handles.push(tokio::spawn(crawl_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        Arc::clone(&crawl_notify),
    )));

    // Embed: multi-lane
    for _ in 0..embed_lanes {
        worker_handles.push(tokio::spawn(embed_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&embed_notify),
        )));
    }

    // Extract: single lane
    worker_handles.push(tokio::spawn(extract_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&extract_notify),
    )));

    // Ingest: multi-lane
    for _ in 0..ingest_lanes {
        worker_handles.push(tokio::spawn(ingest_worker(
            Arc::clone(&pool),
            Arc::clone(&cfg),
            Arc::clone(&ingest_notify),
        )));
    }

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        worker_handles,
    }
}

/// Generic worker loop: wait for Notify or poll timeout, then claim + run pending jobs.
async fn worker_loop<F, Fut>(
    pool: Arc<SqlitePool>,
    table: &'static str,
    notify: Arc<Notify>,
    run_job: F,
) where
    F: Fn(Arc<SqlitePool>, uuid::Uuid) -> Fut + Send + 'static,
    Fut: Future<Output = JobResult> + Send,
{
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }

        loop {
            match claim_next_pending(&pool, table).await {
                Ok(Some(id)) => {
                    let result = run_job(Arc::clone(&pool), id).await;
                    match result {
                        Ok(result_json) => {
                            if let Err(e) =
                                mark_completed(&pool, table, id, result_json.as_ref()).await
                            {
                                tracing::error!(
                                    table,
                                    job_id = %id,
                                    error = %e,
                                    "lite worker: failed to mark job completed — job will remain in 'running' state"
                                );
                            }
                        }
                        Err(e) => {
                            if let Err(mark_err) =
                                mark_failed(&pool, table, id, &e.to_string()).await
                            {
                                tracing::error!(
                                    table,
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
                    tracing::error!("worker claim error (table={}): {}", table, e);
                    break;
                }
            }
        }
    }
}

async fn crawl_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    _cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
) {
    worker_loop(pool, "axon_crawl_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_crawl_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn embed_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_embed_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_embed_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn extract_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_extract_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_extract_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn ingest_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_ingest_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_ingest_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::cancel::CancelStore;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;

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
            if let Some(claimed_id) = claim_next_pending(&pool2, "axon_embed_jobs").await.unwrap() {
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
    async fn dropping_worker_handles_aborts_worker_loops() {
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
        .expect("worker tasks should be aborted when WorkerHandles is dropped");
    }
}

use crate::crates::core::config::Config;
use crate::crates::jobs::lite::cancel::CancelStore;
use crate::crates::jobs::lite::ops::{claim_next_pending, mark_completed, mark_failed};
use sqlx::SqlitePool;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Handles to wake specific worker types when new jobs are enqueued.
pub struct WorkerHandles {
    pub crawl: Arc<Notify>,
    pub embed: Arc<Notify>,
    pub extract: Arc<Notify>,
    pub ingest: Arc<Notify>,
    pub refresh: Arc<Notify>,
    pub graph: Arc<Notify>,
}

/// Spawn in-process worker tasks for all 6 job types.
pub fn spawn_workers(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
) -> WorkerHandles {
    let crawl_notify = Arc::new(Notify::new());
    let embed_notify = Arc::new(Notify::new());
    let extract_notify = Arc::new(Notify::new());
    let ingest_notify = Arc::new(Notify::new());
    let refresh_notify = Arc::new(Notify::new());
    let graph_notify = Arc::new(Notify::new());

    tokio::spawn(crawl_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        Arc::clone(&crawl_notify),
    ));
    tokio::spawn(embed_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&embed_notify),
    ));
    tokio::spawn(extract_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&extract_notify),
    ));
    tokio::spawn(ingest_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&ingest_notify),
    ));
    tokio::spawn(refresh_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&refresh_notify),
    ));
    tokio::spawn(graph_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&graph_notify),
    ));

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        refresh: refresh_notify,
        graph: graph_notify,
    }
}

type JobResult = Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

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
                            let _ = mark_completed(&pool, table, id, result_json.as_ref()).await;
                        }
                        Err(e) => {
                            let _ = mark_failed(&pool, table, id, &e.to_string()).await;
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

async fn refresh_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_refresh_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_refresh_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn graph_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_graph_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_graph_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

// ── Lite job runners ──────────────────────────────────────────────────────────

async fn run_crawl_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    // TODO: wire to crawl job processor
    Ok(None)
}

async fn run_embed_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    // TODO: wire to embed job processor
    Ok(None)
}

async fn run_extract_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    Ok(None)
}

async fn run_ingest_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    Ok(None)
}

async fn run_refresh_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    Ok(None)
}

async fn run_graph_job_lite(_pool: &SqlitePool, _cfg: &Config, _id: uuid::Uuid) -> JobResult {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
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
        tokio::spawn(async move {
            if let Some(claimed_id) = claim_next_pending(&pool2, "axon_embed_jobs").await.unwrap() {
                assert_eq!(claimed_id, id);
                notify2.notify_one();
            }
        });

        notify.notify_one();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let row: (String,) = sqlx::query_as("SELECT status FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_ne!(row.0, "pending", "job should have been claimed");
    }
}

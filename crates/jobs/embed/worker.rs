use super::*;
use crate::crates::core::http::validate_url;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::jobs::graph::enqueue_graph_job;
use crate::crates::jobs::worker_lane::{
    ProcessFn, WorkerConfig, resolve_lane_count, run_job_worker, validate_worker_env_vars,
};
use crate::crates::vector::ops::{EmbedProgress, embed_path_native_with_progress};
use futures_util::future::LocalBoxFuture;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;

/// Debounce interval for embed progress updates to Postgres.
/// Matches the crawl worker's 500ms debounce to avoid excessive writes.
const PROGRESS_DEBOUNCE_MS: u64 = 500;

/// Open a Redis connection for embed cancel checks. Returns None (with warning)
/// on failure — cancel checks will be skipped (fail-safe: never false-cancel).
async fn open_embed_redis(cfg: &Config) -> Option<redis::aio::MultiplexedConnection> {
    let client = match redis::Client::open(cfg.redis_url.clone()) {
        Ok(c) => c,
        Err(e) => {
            log_warn(&format!("embed cancel redis client open failed: {e}"));
            return None;
        }
    };
    match tokio::time::timeout(
        Duration::from_secs(EMBED_CANCEL_REDIS_TIMEOUT_SECS),
        client.get_multiplexed_async_connection(),
    )
    .await
    {
        Ok(Ok(conn)) => Some(conn),
        Ok(Err(e)) => {
            log_warn(&format!("embed cancel redis connect failed: {e}"));
            None
        }
        Err(_) => {
            log_warn(&format!(
                "embed cancel redis connect timeout after {}s",
                EMBED_CANCEL_REDIS_TIMEOUT_SECS
            ));
            None
        }
    }
}

/// Check if the embed job has been canceled via Redis. Returns `true` if a cancel
/// key is present and the job has been marked canceled in the DB, `false` otherwise.
/// If `redis_conn` is None (Redis unavailable), returns `false` (fail-safe).
async fn check_embed_canceled(
    redis_conn: &mut Option<redis::aio::MultiplexedConnection>,
    pool: &PgPool,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    let Some(conn) = redis_conn.as_mut() else {
        return Ok(false);
    };
    let cancel_key = format!("axon:embed:cancel:{id}");
    let cancel_value: Option<String> = match tokio::time::timeout(
        Duration::from_secs(EMBED_CANCEL_REDIS_TIMEOUT_SECS),
        conn.get::<_, Option<String>>(&cancel_key),
    )
    .await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            log_warn(&format!("embed cancel check failed for {id}: {e}"));
            // Clear the shared slot so the next job can attempt to re-establish
            // the connection rather than retrying a known-broken one.
            *redis_conn = None;
            None
        }
        Err(_) => {
            log_warn(&format!(
                "embed cancel check timeout for {id} after {}s",
                EMBED_CANCEL_REDIS_TIMEOUT_SECS
            ));
            // Treat a timeout as a broken connection — clear the slot so it
            // will be re-opened on the next job attempt.
            *redis_conn = None;
            None
        }
    };
    if cancel_value.is_none() {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE axon_embed_jobs SET status=$2,updated_at=NOW(),finished_at=NOW() WHERE id=$1 AND status IN ($3,$4)",
    )
    .bind(id)
    .bind(JobStatus::Canceled.as_str())
    .bind(JobStatus::Pending.as_str())
    .bind(JobStatus::Running.as_str())
    .execute(pool)
    .await?;
    Ok(true)
}

/// Run the embed operation and return the result JSON. Spawns a progress task
/// to stream intermediate updates to the DB while the embed runs.
async fn run_embed_core(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
    input_text: String,
    collection: String,
    source_type: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<EmbedProgress>(256);
    let progress_pool = pool.clone();
    let progress_task = tokio::spawn(async move {
        let mut last_update = Instant::now() - Duration::from_secs(10); // ensure first fires
        let mut last_progress: Option<EmbedProgress> = None;
        while let Some(progress) = progress_rx.recv().await {
            last_progress = Some(progress);
            if last_update.elapsed() >= Duration::from_millis(PROGRESS_DEBOUNCE_MS)
                && let Some(ref p) = last_progress
            {
                let progress_json = serde_json::json!({
                    "phase": "embedding",
                    "docs_total": p.docs_total,
                    "docs_completed": p.docs_completed,
                    "chunks_embedded": p.chunks_embedded,
                });
                let _ = sqlx::query(
                    "UPDATE axon_embed_jobs SET updated_at=NOW(), result_json=$2 WHERE id=$1 AND status=$3",
                )
                .bind(id)
                .bind(progress_json)
                .bind(JobStatus::Running.as_str())
                .execute(&progress_pool)
                .await;
                last_update = Instant::now();
            }
        }
        // Always flush the final progress on channel close (job completion).
        if let Some(ref p) = last_progress {
            let progress_json = serde_json::json!({
                "phase": "embedding",
                "docs_total": p.docs_total,
                "docs_completed": p.docs_completed,
                "chunks_embedded": p.chunks_embedded,
            });
            let _ = sqlx::query(
                "UPDATE axon_embed_jobs SET updated_at=NOW(), result_json=$2 WHERE id=$1 AND status=$3",
            )
            .bind(id)
            .bind(progress_json)
            .bind(JobStatus::Running.as_str())
            .execute(&progress_pool)
            .await;
        }
    });
    let mut embed_cfg = cfg.clone();
    embed_cfg.collection = collection.clone();
    let summary_result =
        embed_path_native_with_progress(&embed_cfg, &input_text, Some(progress_tx), source_type)
            .await;
    if let Err(err) = progress_task.await {
        log_warn(&format!(
            "embed progress_task panicked for job {id}: {err:?}"
        ));
    }
    let summary = summary_result?;
    Ok(serde_json::json!({
        "input": input_text,
        "collection": collection,
        "docs_embedded": summary.docs_embedded,
        "chunks_embedded": summary.chunks_embedded,
        "source": "rust"
    }))
}

#[cfg(test)]
pub(crate) async fn process_embed_job_with_runner<F>(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
    runner: F,
) -> Result<(), Box<dyn Error>>
where
    F: for<'a> FnOnce(
        &'a Config,
        &'a PgPool,
        Uuid,
        String,
        String,
        Option<&'a str>,
    ) -> LocalBoxFuture<'a, Result<serde_json::Value, Box<dyn Error>>>,
{
    // Fallback: open per-job when called without a shared connection (e.g., tests).
    let mut redis_conn = open_embed_redis(cfg).await;
    process_embed_job_with_runner_and_redis(cfg, pool, id, &mut redis_conn, runner).await
}

/// Inner implementation that accepts a pre-opened Redis connection.
/// The worker passes a shared connection opened once at startup; the public
/// `process_embed_job_with_runner` opens a fresh one for CLI/test callers.
async fn process_embed_job_with_runner_and_redis<F>(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
    redis_conn: &mut Option<redis::aio::MultiplexedConnection>,
    runner: F,
) -> Result<(), Box<dyn Error>>
where
    F: for<'a> FnOnce(
        &'a Config,
        &'a PgPool,
        Uuid,
        String,
        String,
        Option<&'a str>,
    ) -> LocalBoxFuture<'a, Result<serde_json::Value, Box<dyn Error>>>,
{
    let job_start = Instant::now();

    let run_result = async {
        let row = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT input_text, config_json FROM axon_embed_jobs WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        let Some((input_text, cfg_json)) = row else {
            return Ok::<Option<serde_json::Value>, Box<dyn Error>>(None);
        };
        let input_preview: String = input_text.chars().take(80).collect();
        log_info(&format!(
            "embed worker started job {id} input={input_preview}"
        ));
        if check_embed_canceled(redis_conn, pool, id).await? {
            return Ok(None);
        }
        let job_cfg: EmbedJobConfig = serde_json::from_value(cfg_json)?;
        let result = runner(
            cfg,
            pool,
            id,
            input_text,
            job_cfg.collection,
            job_cfg.source_type.as_deref(),
        )
        .await?;
        Ok(Some(result))
    }
    .await;
    // Convert Box<dyn Error> to String before the match so no !Send type
    // is held across any await inside the match arms (tokio::spawn Send bound).
    let run_result = run_result.map_err(|e| e.to_string());

    match run_result {
        Ok(Some(result_json)) => {
            let chunk_count = result_json["chunks_embedded"].as_u64().unwrap_or(0);
            let collection = result_json["collection"].as_str().unwrap_or("").to_string();
            let input = result_json["input"].as_str().unwrap_or("").to_string();
            sqlx::query(
                "UPDATE axon_embed_jobs \
                 SET status=$2,updated_at=NOW(),finished_at=NOW(),result_json=$3,error_text=NULL \
                 WHERE id=$1 AND status=$4",
            )
            .bind(id)
            .bind(JobStatus::Completed.as_str())
            .bind(result_json)
            .bind(JobStatus::Running.as_str())
            .execute(pool)
            .await?;
            if !cfg.neo4j_url.trim().is_empty()
                && validate_url(&input).is_ok()
                && let Err(err) = enqueue_graph_job(pool, cfg, &input, "embed").await
            {
                log_warn(&format!("graph auto-enqueue failed for {input}: {err}"));
            }
            log_done(&format!(
                "worker completed embed job {id} collection={collection} chunk_count={chunk_count} duration_ms={}",
                job_start.elapsed().as_millis()
            ));
        }
        Ok(None) => {}
        Err(error_text) => {
            if let Err(e) = mark_job_failed(pool, TABLE, id, &error_text).await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e}"));
            }
            log_warn(&format!("worker failed embed job {id}: {error_text}"));
        }
    }

    Ok(())
}

async fn process_embed_job(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
    redis_conn: &mut Option<redis::aio::MultiplexedConnection>,
) -> Result<(), Box<dyn Error>> {
    process_embed_job_with_runner_and_redis(
        cfg,
        pool,
        id,
        redis_conn,
        |cfg, pool, id, input_text, collection, source_type| {
            Box::pin(run_embed_core(
                cfg,
                pool,
                id,
                input_text,
                collection,
                source_type,
            ))
        },
    )
    .await
}

async fn process_claimed_embed_job(
    cfg: Arc<Config>,
    pool: PgPool,
    id: Uuid,
    shared_redis: Arc<tokio::sync::Mutex<Option<redis::aio::MultiplexedConnection>>>,
) {
    let _job_span = tracing::info_span!("embed_job", job_id = %id).entered();
    // Clone the shared connection for this job. MultiplexedConnection is Clone
    // (internally Arc-wrapped), so this is a cheap reference count bump — not
    // a new TCP connection. If the shared slot is None (Redis was down at
    // startup or a prior failure), attempt to re-establish it now so cancel
    // checks become available without requiring a worker restart.
    let mut redis_conn = {
        let mut guard = shared_redis.lock().await;
        if guard.is_none() {
            *guard = open_embed_redis(&cfg).await;
        }
        guard.clone()
    };
    let fail_msg = match process_embed_job(&cfg, &pool, id, &mut redis_conn).await {
        Ok(()) => None,
        Err(err) => Some(err.to_string()),
    };
    if let Some(error_text) = fail_msg {
        if let Err(e) = mark_job_failed(&pool, TABLE, id, &error_text).await {
            log_warn(&format!("mark_job_failed failed job_id={id} error={e}"));
        }
        log_warn(&format!("worker failed embed job {id}: {error_text}"));
    }
}

pub async fn run_embed_worker(cfg: &Config) -> anyhow::Result<()> {
    // Validate required environment variables before attempting any connections.
    if let Err(msg) = validate_worker_env_vars() {
        return Err(anyhow::anyhow!("{msg}"));
    }

    log_info(&format!(
        "worker_start worker=embed queue={} collection={}",
        cfg.embed_queue, cfg.collection
    ));

    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;

    // Open a single Redis connection at worker startup. All job lanes clone this
    // connection (cheap Arc bump) instead of opening a new TCP connection per job.
    // Wrapped in Arc<Mutex> so lanes can safely share the Option (reconnect on None).
    let shared_redis = Arc::new(tokio::sync::Mutex::new(open_embed_redis(cfg).await));

    let wc = WorkerConfig {
        table: TABLE,
        queue_name: cfg.embed_queue.clone(),
        job_kind: "embed",
        consumer_tag_prefix: "axon-rust-embed-worker",
        lane_count: resolve_lane_count("AXON_EMBED_LANES", 2, 32),
        heartbeat_interval_secs: EMBED_HEARTBEAT_INTERVAL_SECS,
    };

    let process_fn: ProcessFn = Arc::new(move |cfg, pool, id| {
        let redis = Arc::clone(&shared_redis);
        Box::pin(process_claimed_embed_job(cfg, pool, id, redis))
    });

    run_job_worker(cfg, pool, &wc, process_fn)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

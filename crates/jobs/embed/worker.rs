use super::*;
use crate::crates::core::http::validate_url;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::jobs::graph::enqueue_graph_job;
use crate::crates::jobs::worker_lane::{
    ProcessFn, WorkerConfig, resolve_lane_count, run_job_worker, validate_worker_env_vars,
};
use crate::crates::vector::ops::{EmbedProgress, embed_path_native_with_progress};
use tokio::time::Duration;

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
            None
        }
        Err(_) => {
            log_warn(&format!(
                "embed cancel check timeout for {id} after {}s",
                EMBED_CANCEL_REDIS_TIMEOUT_SECS
            ));
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
) -> Result<serde_json::Value, Box<dyn Error>> {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<EmbedProgress>(256);
    let progress_pool = pool.clone();
    let progress_task = tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let progress_json = serde_json::json!({
                "phase": "embedding",
                "docs_total": progress.docs_total,
                "docs_completed": progress.docs_completed,
                "chunks_embedded": progress.chunks_embedded,
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
        embed_path_native_with_progress(&embed_cfg, &input_text, Some(progress_tx)).await;
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

async fn process_embed_job(cfg: &Config, pool: &PgPool, id: Uuid) -> Result<(), Box<dyn Error>> {
    // Open a single Redis connection for cancel checks (reused across the job lifecycle).
    let mut redis_conn = open_embed_redis(cfg).await;
    let job_start = std::time::Instant::now();

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
        if check_embed_canceled(&mut redis_conn, pool, id).await? {
            return Ok(None);
        }
        let job_cfg: EmbedJobConfig = serde_json::from_value(cfg_json)?;
        let result = run_embed_core(cfg, pool, id, input_text, job_cfg.collection).await?;
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

async fn process_claimed_embed_job(cfg: Config, pool: PgPool, id: Uuid) {
    let fail_msg = match process_embed_job(&cfg, &pool, id).await {
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
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }

    let wc = WorkerConfig {
        table: TABLE,
        queue_name: cfg.embed_queue.clone(),
        job_kind: "embed",
        consumer_tag_prefix: "axon-rust-embed-worker",
        lane_count: resolve_lane_count("AXON_EMBED_LANES", 2, 32),
        heartbeat_interval_secs: EMBED_HEARTBEAT_INTERVAL_SECS,
    };

    let process_fn: ProcessFn =
        std::sync::Arc::new(|cfg, pool, id| Box::pin(process_claimed_embed_job(cfg, pool, id)));

    run_job_worker(cfg, pool, &wc, process_fn)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

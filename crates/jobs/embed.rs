use crate::crates::cli::commands::common::truncate_chars;
use crate::crates::core::config::Config;
use crate::crates::core::health::redis_healthy;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::jobs::common::{
    JobTable, begin_schema_migration_tx, enqueue_job, make_pool, mark_job_failed,
    open_amqp_connection_and_channel, purge_queue_safe, reclaim_stale_running_jobs,
};
use crate::crates::jobs::status::JobStatus;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

static SCHEMA_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

const TABLE: JobTable = JobTable::Embed;
const EMBED_HEARTBEAT_INTERVAL_SECS: u64 = 15;
const EMBED_CANCEL_REDIS_TIMEOUT_SECS: u64 = 3;
const EMBED_SCHEMA_LOCK_KEY: i64 = 0xA804_0002;

mod worker;
pub use worker::run_embed_worker;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmbedJobConfig {
    collection: String,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct EmbedJob {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub input_text: String,
    pub result_json: Option<serde_json::Value>,
    pub config_json: serde_json::Value,
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, EMBED_SCHEMA_LOCK_KEY).await?;

    {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_embed_jobs (
                id UUID PRIMARY KEY,
                status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                started_at TIMESTAMPTZ,
                finished_at TIMESTAMPTZ,
                error_text TEXT,
                input_text TEXT NOT NULL,
                result_json JSONB,
                config_json JSONB NOT NULL
            )
            "#,
        )
        .execute(&mut *tx)
        .await?;

        // Composite partial index for claim_next_pending: WHERE status='pending' ORDER BY created_at
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_axon_embed_jobs_pending ON axon_embed_jobs(created_at ASC) WHERE status = 'pending'"
        )
        .execute(&mut *tx)
        .await?;

        // Add CHECK constraint to existing tables (idempotent via IF NOT EXISTS pattern).
        sqlx::query(
            r#"DO $$ BEGIN
                ALTER TABLE axon_embed_jobs ADD CONSTRAINT axon_embed_jobs_status_check
                    CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled'));
            EXCEPTION WHEN duplicate_object THEN NULL;
            END $$"#,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Start an embed job, creating a new pool for this call (CLI / one-shot use).
pub async fn start_embed_job(cfg: &Config, input: &str) -> Result<Uuid, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    start_embed_job_with_pool(&pool, cfg, input).await
}

/// Start an embed job using a pre-existing pool. Used by workers that already
/// hold a long-lived pool to avoid per-call TCP connection churn.
pub(crate) async fn start_embed_job_with_pool(
    pool: &PgPool,
    cfg: &Config,
    input: &str,
) -> Result<Uuid, Box<dyn Error>> {
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(pool).await?;
        let _ = SCHEMA_INIT.set(());
    }

    let cfg_json = serde_json::to_value(EmbedJobConfig {
        collection: cfg.collection.clone(),
    })?;
    let running_fresh_secs = cfg.watchdog_stale_timeout_secs.max(30).min(i32::MAX as i64) as i32;
    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM axon_embed_jobs
        WHERE (
                status = $3
             OR (status = $4 AND updated_at >= NOW() - make_interval(secs => $5::int))
        )
          AND input_text = $1
          AND config_json = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(input)
    .bind(cfg_json.clone())
    .bind(JobStatus::Pending.as_str())
    .bind(JobStatus::Running.as_str())
    .bind(running_fresh_secs)
    .fetch_optional(pool)
    .await?
    {
        log_info(&format!(
            "embed dedupe hit: reusing active job {} for input ({}B): {}",
            existing_id,
            input.len(),
            truncate_chars(input, 80)
        ));
        return Ok(existing_id);
    }
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO axon_embed_jobs (id, status, input_text, config_json) VALUES ($1, $2, $3, $4)"#,
    )
    .bind(id)
    .bind(JobStatus::Pending.as_str())
    .bind(input)
    .bind(cfg_json)
    .execute(pool)
    .await?;

    if let Err(err) = enqueue_job(cfg, &cfg.embed_queue, id).await {
        log_warn(&format!(
            "embed enqueue failed for {id}; polling fallback will pick up: {err}"
        ));
    }

    Ok(id)
}

pub async fn get_embed_job(cfg: &Config, id: Uuid) -> Result<Option<EmbedJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    Ok(sqlx::query_as::<_, EmbedJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,input_text,result_json,config_json FROM axon_embed_jobs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?)
}
pub async fn list_embed_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<EmbedJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    Ok(sqlx::query_as::<_, EmbedJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,input_text,result_json,config_json FROM axon_embed_jobs ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?)
}

pub async fn cancel_embed_job(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    let rows = sqlx::query(
        "UPDATE axon_embed_jobs \
         SET status=$2,updated_at=NOW(),finished_at=NOW() \
         WHERE id=$1 AND status IN ($3,$4)",
    )
    .bind(id)
    .bind(JobStatus::Canceled.as_str())
    .bind(JobStatus::Pending.as_str())
    .bind(JobStatus::Running.as_str())
    .execute(&pool)
    .await?
    .rows_affected();

    if rows > 0 {
        // Redis cancel signal is best-effort: DB update already succeeded,
        // so we log a warning but do NOT propagate Redis errors.
        match redis::Client::open(cfg.redis_url.clone()) {
            Ok(redis_client) => {
                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(3),
                    redis_client.get_multiplexed_async_connection(),
                )
                .await
                {
                    Ok(Ok(mut conn)) => {
                        let key = format!("axon:embed:cancel:{id}");
                        if let Err(e) = conn.set_ex::<_, _, ()>(key, "1", 86400).await {
                            log_warn(&format!(
                                "embed cancel: Redis SET failed for job {id} (DB already updated): {e}"
                            ));
                        }
                    }
                    Ok(Err(e)) => {
                        log_warn(&format!(
                            "embed cancel: Redis connect failed for job {id} (DB already updated): {e}"
                        ));
                    }
                    Err(_) => {
                        log_warn(&format!(
                            "embed cancel: Redis connect timeout for job {id} after 3s (DB already updated)"
                        ));
                    }
                }
            }
            Err(e) => {
                log_warn(&format!(
                    "embed cancel: Redis client open failed for job {id} (DB already updated): {e}"
                ));
            }
        }
    }
    Ok(rows > 0)
}

pub async fn cleanup_embed_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    let deleted = sqlx::query("DELETE FROM axon_embed_jobs WHERE status IN ($1,$2)")
        .bind(JobStatus::Failed.as_str())
        .bind(JobStatus::Canceled.as_str())
        .execute(&pool)
        .await?
        .rows_affected();
    Ok(deleted)
}

pub async fn clear_embed_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    let rows = sqlx::query("DELETE FROM axon_embed_jobs")
        .execute(&pool)
        .await?
        .rows_affected();
    if let Err(e) = purge_queue_safe(cfg, &cfg.embed_queue).await {
        log_warn(&format!(
            "queue_purge_failed queue={} error={e}",
            cfg.embed_queue
        ));
    }
    Ok(rows)
}

pub async fn recover_stale_embed_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(&pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    let stats = reclaim_stale_running_jobs(
        &pool,
        TABLE,
        "embed",
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        "manual",
    )
    .await?;
    Ok(stats.reclaimed_jobs)
}

pub async fn embed_doctor(cfg: &Config) -> Result<serde_json::Value, String> {
    let pg_ok = make_pool(cfg).await.is_ok();
    let amqp_ok = match open_amqp_connection_and_channel(cfg, &cfg.embed_queue).await {
        Ok((conn, ch)) => {
            if let Err(e) = ch.close(0, "probe".into()).await {
                log_warn(&format!(
                    "amqp ch_close failed queue={} error={e}",
                    cfg.embed_queue
                ));
            }
            if let Err(e) = conn.close(200, "probe".into()).await {
                log_warn(&format!(
                    "amqp conn_close failed queue={} error={e}",
                    cfg.embed_queue
                ));
            }
            true
        }
        Err(_) => false,
    };
    let redis_ok = redis_healthy(&cfg.redis_url).await;
    Ok(serde_json::json!({
        "postgres_ok": pg_ok,
        "amqp_ok": amqp_ok,
        "redis_ok": redis_ok,
        "tei_configured": !cfg.tei_url.is_empty(),
        "qdrant_configured": !cfg.qdrant_url.is_empty(),
        "all_ok": pg_ok && amqp_ok && redis_ok
    }))
}

#[cfg(test)]
mod tests;

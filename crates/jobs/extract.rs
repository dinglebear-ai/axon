//! Extract job schema uses advisory-lock DDL via `common::schema::begin_schema_migration_tx`.
//! See `common/schema.rs` for the canonical pattern.

use crate::crates::core::config::Config;
use crate::crates::core::content::{
    DeterministicExtractionEngine, ExtractWebConfig, run_extract_with_engine,
};
use crate::crates::core::health::redis_healthy;
use crate::crates::core::logging::{log_debug, log_done, log_info, log_warn};
use crate::crates::jobs::common::{
    JobTable, begin_schema_migration_tx, enqueue_job, make_pool, mark_job_failed,
    open_amqp_channel, purge_queue_safe, reclaim_stale_running_jobs,
};
use crate::crates::jobs::status::JobStatus;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

const TABLE: JobTable = JobTable::Extract;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtractJobConfig {
    prompt: Option<String>,
    max_pages: u32,
}

#[derive(Debug, FromRow, Serialize)]
pub struct ExtractJob {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub urls_json: serde_json::Value,
    pub result_json: Option<serde_json::Value>,
}

/// Advisory lock key for extract job schema migrations (unique per table).
const EXTRACT_SCHEMA_LOCK_KEY: i64 = 0x6578_7472_6163_7400_i64;

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, EXTRACT_SCHEMA_LOCK_KEY).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_extract_jobs (
            id UUID PRIMARY KEY,
            status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled')),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            started_at TIMESTAMPTZ,
            finished_at TIMESTAMPTZ,
            error_text TEXT,
            urls_json JSONB NOT NULL,
            result_json JSONB,
            config_json JSONB NOT NULL
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_extract_jobs_pending ON axon_extract_jobs(created_at ASC) WHERE status = 'pending'"
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"DO $$ BEGIN
            ALTER TABLE axon_extract_jobs ADD CONSTRAINT axon_extract_jobs_status_check
                CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled'));
        EXCEPTION WHEN duplicate_object THEN NULL;
        END $$"#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Start an extract job, creating a new pool for this call (CLI / one-shot use).
pub async fn start_extract_job(
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
) -> Result<Uuid, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    start_extract_job_with_pool(&pool, cfg, urls, prompt).await
}

/// Start an extract job using a pre-existing pool. Used by workers that already
/// hold a long-lived pool to avoid per-call TCP connection churn.
pub(crate) async fn start_extract_job_with_pool(
    pool: &PgPool,
    cfg: &Config,
    urls: &[String],
    prompt: Option<String>,
) -> Result<Uuid, Box<dyn Error>> {
    ensure_schema(pool).await?;

    let urls_json = serde_json::to_value(urls)?;
    let cfg_json = serde_json::to_value(ExtractJobConfig {
        prompt,
        max_pages: cfg.max_pages,
    })?;
    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM axon_extract_jobs
        WHERE status = ANY($3)
          AND urls_json = $1
          AND config_json = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(urls_json.clone())
    .bind(cfg_json.clone())
    .bind(vec![
        JobStatus::Pending.as_str(),
        JobStatus::Running.as_str(),
    ])
    .fetch_optional(pool)
    .await?
    {
        log_info(&format!(
            "extract dedupe hit: reusing active job {} for {} urls",
            existing_id,
            urls.len()
        ));
        return Ok(existing_id);
    }
    let id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO axon_extract_jobs (id, status, urls_json, config_json) VALUES ($1, $2, $3, $4)"#,
    )
    .bind(id)
    .bind(JobStatus::Pending.as_str())
    .bind(urls_json)
    .bind(cfg_json)
    .execute(pool)
    .await?;

    if let Err(err) = enqueue_job(cfg, &cfg.extract_queue, id).await {
        log_warn(&format!(
            "extract enqueue failed for {id}; polling fallback will pick up: {err}"
        ));
    }

    Ok(id)
}

pub async fn get_extract_job(cfg: &Config, id: Uuid) -> Result<Option<ExtractJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    Ok(sqlx::query_as::<_, ExtractJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,urls_json,result_json FROM axon_extract_jobs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?)
}
pub async fn list_extract_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<ExtractJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    Ok(sqlx::query_as::<_, ExtractJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,urls_json,result_json FROM axon_extract_jobs ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?)
}

pub async fn cancel_extract_job(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let rows = sqlx::query(
        "UPDATE axon_extract_jobs SET status=$2,updated_at=NOW(),finished_at=NOW() WHERE id=$1 AND status = ANY($3)",
    )
    .bind(id)
    .bind(JobStatus::Canceled.as_str())
    .bind(vec![
        JobStatus::Pending.as_str(),
        JobStatus::Running.as_str(),
    ])
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
                        let key = format!("axon:extract:cancel:{id}");
                        if let Err(e) = conn.set_ex::<_, _, ()>(key, "1", 86400).await {
                            log_warn(&format!(
                                "extract cancel: Redis SET failed for job {id} (DB already updated): {e}"
                            ));
                        }
                    }
                    Ok(Err(e)) => {
                        log_warn(&format!(
                            "extract cancel: Redis connect failed for job {id} (DB already updated): {e}"
                        ));
                    }
                    Err(_) => {
                        log_warn(&format!(
                            "extract cancel: Redis connect timeout for job {id} after 3s (DB already updated)"
                        ));
                    }
                }
            }
            Err(e) => {
                log_warn(&format!(
                    "extract cancel: Redis client open failed for job {id} (DB already updated): {e}"
                ));
            }
        }
    }
    Ok(rows > 0)
}

pub async fn cleanup_extract_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let mut total = 0u64;
    loop {
        let deleted = sqlx::query(
            "DELETE FROM axon_extract_jobs WHERE id IN (
                SELECT id FROM axon_extract_jobs
                WHERE status = ANY($1)
                LIMIT 1000
            )",
        )
        .bind(vec![
            JobStatus::Failed.as_str(),
            JobStatus::Canceled.as_str(),
        ])
        .execute(&pool)
        .await?
        .rows_affected();
        total += deleted;
        if deleted == 0 {
            break;
        }
    }
    Ok(total)
}

pub async fn clear_extract_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let rows = sqlx::query("DELETE FROM axon_extract_jobs")
        .execute(&pool)
        .await?
        .rows_affected();
    if let Err(e) = purge_queue_safe(cfg, &cfg.extract_queue).await {
        log_warn(&format!(
            "queue_purge_failed queue={} error={e}",
            cfg.extract_queue
        ));
    }
    Ok(rows)
}

mod worker;

pub async fn run_extract_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    worker::run_extract_worker(cfg).await
}

pub async fn recover_stale_extract_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let stats = reclaim_stale_running_jobs(
        &pool,
        TABLE,
        "extract",
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        "manual",
    )
    .await?;
    Ok(stats.reclaimed_jobs)
}

pub async fn extract_doctor(cfg: &Config) -> Result<serde_json::Value, String> {
    let pg_ok = make_pool(cfg).await.is_ok();
    let amqp_ok = open_amqp_channel(cfg, &cfg.extract_queue).await.is_ok();
    let redis_ok = redis_healthy(&cfg.redis_url).await;
    Ok(serde_json::json!({
        "postgres_ok": pg_ok,
        "amqp_ok": amqp_ok,
        "redis_ok": redis_ok,
        "all_ok": pg_ok && amqp_ok && redis_ok
    }))
}

#[cfg(test)]
mod tests;

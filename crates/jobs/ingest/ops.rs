use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::jobs::common::{enqueue_job, make_pool, purge_queue_safe};
use crate::crates::jobs::status::JobStatus;
use sqlx::PgPool;
use std::error::Error;
use uuid::Uuid;

use super::schema::ensure_schema;
use super::types::{IngestJob, IngestJobConfig, IngestSource, source_type_label, target_label};

pub async fn start_ingest_job(cfg: &Config, source: IngestSource) -> Result<Uuid, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let job_config = IngestJobConfig {
        source: source.clone(),
        collection: cfg.collection.clone(),
    };
    let cfg_json = serde_json::to_value(&job_config)?;
    let source_type = source_type_label(&source);
    let target = target_label(&source);

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO axon_ingest_jobs (id, status, source_type, target, config_json) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(JobStatus::Pending.as_str())
    .bind(source_type)
    .bind(&target)
    .bind(cfg_json)
    .execute(&pool)
    .await?;

    if let Err(err) = enqueue_job(cfg, &cfg.ingest_queue, id).await {
        log_warn(&format!(
            "ingest enqueue failed for {id}; polling fallback will pick up: {err}"
        ));
    }

    log_info(&format!(
        "ingest job queued: id={id} source={source_type} target={target}"
    ));
    Ok(id)
}

pub async fn get_ingest_job(cfg: &Config, id: Uuid) -> Result<Option<IngestJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    Ok(sqlx::query_as::<_, IngestJob>(
        "SELECT id,status,source_type,target,created_at,updated_at,started_at,finished_at,\
         error_text,result_json,config_json FROM axon_ingest_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?)
}

pub async fn list_ingest_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<IngestJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    Ok(sqlx::query_as::<_, IngestJob>(
        "SELECT id,status,source_type,target,created_at,updated_at,started_at,finished_at,\
         error_text,result_json,config_json FROM axon_ingest_jobs \
         ORDER BY CASE status \
           WHEN 'running' THEN 0 \
           WHEN 'pending' THEN 1 \
           WHEN 'failed'  THEN 2 \
           WHEN 'completed' THEN 3 \
           ELSE 4 \
         END, created_at DESC \
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?)
}

pub async fn cancel_ingest_job(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let rows = sqlx::query(
        "UPDATE axon_ingest_jobs SET status=$2,updated_at=NOW(),finished_at=NOW() \
         WHERE id=$1 AND status = ANY($3)",
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
    Ok(rows > 0)
}

pub async fn cleanup_ingest_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    Ok(sqlx::query(
        "DELETE FROM axon_ingest_jobs WHERE status = ANY($1) \
         OR (status = $2 AND finished_at < NOW() - INTERVAL '30 days')",
    )
    .bind(vec![
        JobStatus::Failed.as_str(),
        JobStatus::Canceled.as_str(),
    ])
    .bind(JobStatus::Completed.as_str())
    .execute(&pool)
    .await?
    .rows_affected())
}

pub async fn clear_ingest_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let rows = sqlx::query("DELETE FROM axon_ingest_jobs")
        .execute(&pool)
        .await?
        .rows_affected();
    if let Err(e) = purge_queue_safe(cfg, &cfg.ingest_queue).await {
        log_warn(&format!(
            "queue_purge_failed queue={} error={e}",
            cfg.ingest_queue
        ));
    }
    Ok(rows)
}

pub(crate) async fn mark_completed(pool: &PgPool, id: Uuid, chunks: usize) {
    use crate::crates::core::logging::log_warn;

    match sqlx::query(
        "UPDATE axon_ingest_jobs SET status=$2,updated_at=NOW(),finished_at=NOW(),\
         result_json=COALESCE(result_json,'{}'::jsonb)||$3 WHERE id=$1 AND status=$4",
    )
    .bind(id)
    .bind(JobStatus::Completed.as_str())
    .bind(serde_json::json!({"chunks_embedded": chunks, "enumerating": false}))
    .bind(JobStatus::Running.as_str())
    .execute(pool)
    .await
    {
        Ok(done) => {
            if done.rows_affected() == 0 {
                log_warn(&format!(
                    "command=ingest_worker completion_update_skipped job_id={id} reason=not_running_state"
                ));
            }
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_worker mark_completed_failed job_id={id} err={e}"
            ));
        }
    }
}

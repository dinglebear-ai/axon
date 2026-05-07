pub mod types;

#[cfg(test)]
mod tests;

pub use self::types::{IngestJob, IngestJobConfig, IngestSource};

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::ingest::types::{source_type_label, target_label};
use crate::jobs::lite::query::ms_to_dt;
use crate::jobs::lite::store::open_sqlite_pool;
use std::error::Error;
use uuid::Uuid;

// SQLite row tuple for axon_ingest_jobs
type IngestJobRow = (
    String,         // id
    String,         // status
    String,         // source_type
    String,         // target
    i64,            // created_at ms
    i64,            // updated_at ms
    Option<i64>,    // started_at ms
    Option<i64>,    // finished_at ms
    Option<String>, // error_text
    Option<String>, // result_json
    String,         // config_json
);

fn row_to_ingest_job(row: IngestJobRow) -> IngestJob {
    let (
        id,
        status,
        source_type,
        target,
        created_at,
        updated_at,
        started_at,
        finished_at,
        error_text,
        result_json,
        config_json,
    ) = row;
    IngestJob {
        id: Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()),
        status,
        source_type,
        target,
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        error_text,
        result_json: result_json.and_then(|s| serde_json::from_str(&s).ok()),
        config_json: config_json
            .parse::<serde_json::Value>()
            .unwrap_or(serde_json::Value::Object(Default::default())),
    }
}

/// Count all ingest jobs in SQLite.
pub async fn count_ingest_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    Ok(crate::jobs::lite::query::count_jobs(&pool, JobKind::Ingest).await?)
}

/// Fetch a single ingest job by UUID from SQLite.
pub async fn get_ingest_job(cfg: &Config, id: Uuid) -> Result<Option<IngestJob>, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let row: Option<IngestJobRow> = sqlx::query_as(
        "SELECT id, status, source_type, target, created_at, updated_at, started_at, \
         finished_at, error_text, result_json, config_json \
         FROM axon_ingest_jobs WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&pool)
    .await?;
    Ok(row.map(row_to_ingest_job))
}

/// List ingest jobs from SQLite with optional source_type filter.
pub async fn list_ingest_jobs(
    cfg: &Config,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<IngestJob>, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let rows: Vec<IngestJobRow> = sqlx::query_as(
        "SELECT id, status, source_type, target, created_at, updated_at, started_at, \
         finished_at, error_text, result_json, config_json \
         FROM axon_ingest_jobs \
         WHERE (?1 IS NULL OR source_type = ?1) \
         ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )
    .bind(source_filter)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    Ok(rows.into_iter().map(row_to_ingest_job).collect())
}

/// Enqueue a new ingest job in SQLite. Returns the new job UUID.
pub async fn start_ingest_job(cfg: &Config, source: IngestSource) -> Result<Uuid, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    let config_json = serde_json::to_string(&source)?;
    Ok(crate::jobs::lite::ops::enqueue_job(
        &pool,
        &JobPayload::Ingest {
            target,
            source_type,
            config_json,
        },
    )
    .await?)
}

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use std::error::Error;
use uuid::Uuid;

use crate::backend::{JobKind, JobPayload};
use crate::store::open_sqlite_pool;
use axon_core::config::Config;

/// Thin Job struct used for CLI status display via `impl_job_status!`.
/// Legacy Postgres runtime has been removed.
/// SQLite operations go through `src/jobs/{store,query,ops}`.
#[derive(Debug, Clone, FromRow, Serialize)]
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

/// Count all extract jobs in SQLite.
pub async fn count_extract_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    Ok(crate::query::count_jobs(&pool, JobKind::Extract).await?)
}

/// Enqueue a new extract job in SQLite. Returns the new job UUID.
pub async fn start_extract_job(
    cfg: &Config,
    urls: Vec<String>,
    prompt: Option<String>,
) -> Result<Uuid, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let config_json = prompt
        .map(|p| serde_json::json!({ "prompt": p }).to_string())
        .unwrap_or_else(|| "{}".to_string());
    Ok(crate::ops::enqueue_job(&pool, &JobPayload::Extract { urls, config_json }, cfg).await?)
}

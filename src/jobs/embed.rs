use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::error::Error;
use uuid::Uuid;

use crate::core::config::Config;
use crate::jobs::backend::JobKind;
use crate::jobs::lite::store::open_sqlite_pool;

/// Thin Job struct used for CLI status display via `impl_job_status!`.
/// Full-mode Postgres runtime has been removed.
/// Lite-mode operations go through `crates/jobs/lite/`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
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

/// Count all embed jobs in SQLite.
pub async fn count_embed_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    Ok(crate::jobs::lite::query::count_jobs(&pool, JobKind::Embed).await?)
}

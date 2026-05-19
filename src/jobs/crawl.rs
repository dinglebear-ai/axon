use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::error::Error;
use uuid::Uuid;

use crate::core::config::Config;
use crate::core::content::canonicalize_url;
use crate::core::http::validate_url;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::store::open_sqlite_pool;

pub mod sitemap;

/// Thin Job struct used for CLI status display via `impl_job_status!`.
/// Legacy Postgres runtime (repo, watchdog, worker) has been removed.
/// SQLite operations go through `src/jobs/{store,query,ops}`.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CrawlJob {
    pub id: Uuid,
    pub status: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub config_json: serde_json::Value,
}

/// Count all crawl jobs in SQLite.
pub async fn count_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    Ok(crate::jobs::query::count_jobs(&pool, JobKind::Crawl).await?)
}

/// Enqueue a new crawl job in SQLite. Returns the new job UUID.
///
/// Validates the URL against the SSRF blocklist and canonicalizes it
/// (strips fragments, normalizes scheme) before persisting.
pub async fn start_crawl_job(cfg: &Config, url: &str) -> Result<Uuid, Box<dyn Error>> {
    validate_url(url)?;
    let canonical = canonicalize_url(url).ok_or("invalid crawl start URL")?;
    let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    Ok(crate::jobs::ops::enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: canonical,
            config_json: "{}".to_string(),
        },
        cfg,
    )
    .await?)
}

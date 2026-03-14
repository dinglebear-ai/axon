use crate::crates::core::config::{Config, RenderMode};
pub(crate) use crate::crates::crawl::manifest::{read_manifest_urls, write_audit_diff};
use crate::crates::jobs::common::{JobTable, begin_schema_migration_tx};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

const TABLE: JobTable = JobTable::Crawl;
pub(super) fn worker_concurrency() -> usize {
    crate::crates::jobs::worker_lane::resolve_lane_count("AXON_CRAWL_WORKER_LANES", 2, 8)
}
const STALE_SWEEP_INTERVAL_SECS: u64 = 30;
const CRAWL_SCHEMA_LOCK_KEY: i64 = 0xA804_0003;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CrawlJobConfig {
    max_pages: u32,
    max_depth: usize,
    include_subdomains: bool,
    #[serde(default = "crate::crates::core::config::parse::excludes::default_exclude_prefixes")]
    exclude_path_prefix: Vec<String>,
    respect_robots: bool,
    min_markdown_chars: usize,
    drop_thin_markdown: bool,
    discover_sitemaps: bool,
    #[serde(default)]
    sitemap_since_days: u32,
    embed: bool,
    render_mode: RenderMode,
    collection: String,
    output_dir: String,
    crawl_concurrency_limit: Option<usize>,
    backfill_concurrency_limit: Option<usize>,
    delay_ms: u64,
    request_timeout_ms: Option<u64>,
    fetch_retries: usize,
    retry_backoff_ms: u64,
    shared_queue: bool,
    extraction_prompt: Option<String>,
    #[serde(default = "default_cache_enabled")]
    cache: bool,
    #[serde(default)]
    cache_skip_browser: bool,
}

fn default_cache_enabled() -> bool {
    true
}

#[derive(Debug, FromRow, Serialize)]
pub struct CrawlJob {
    pub id: Uuid,
    pub url: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
}

fn to_job_config(cfg: &Config) -> CrawlJobConfig {
    CrawlJobConfig {
        max_pages: cfg.max_pages,
        max_depth: cfg.max_depth,
        include_subdomains: cfg.include_subdomains,
        exclude_path_prefix: cfg.exclude_path_prefix.clone(),
        respect_robots: cfg.respect_robots,
        min_markdown_chars: cfg.min_markdown_chars,
        drop_thin_markdown: cfg.drop_thin_markdown,
        discover_sitemaps: cfg.discover_sitemaps,
        sitemap_since_days: cfg.sitemap_since_days,
        embed: cfg.embed,
        render_mode: cfg.render_mode,
        collection: cfg.collection.clone(),
        output_dir: cfg.output_dir.to_string_lossy().to_string(),
        crawl_concurrency_limit: cfg.crawl_concurrency_limit,
        backfill_concurrency_limit: cfg.backfill_concurrency_limit,
        delay_ms: cfg.delay_ms,
        request_timeout_ms: cfg.request_timeout_ms,
        fetch_retries: cfg.fetch_retries,
        retry_backoff_ms: cfg.retry_backoff_ms,
        shared_queue: cfg.shared_queue,
        extraction_prompt: cfg.query.clone(),
        cache: cfg.cache,
        cache_skip_browser: cfg.cache_skip_browser,
    }
}

fn resolve_initial_mode(render_mode: RenderMode, cache_skip_browser: bool) -> RenderMode {
    if cache_skip_browser {
        return RenderMode::Http;
    }
    match render_mode {
        RenderMode::AutoSwitch => RenderMode::Http,
        mode => mode,
    }
}

mod db;
mod robots;
mod worker;
use worker::reclaim_stale_running_jobs;

async fn latest_completed_result_for_url(
    pool: &PgPool,
    url: &str,
    current_job_id: Uuid,
) -> Result<Option<(Uuid, serde_json::Value)>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        r#"
        SELECT id, result_json
        FROM axon_crawl_jobs
        WHERE url = $1
          AND id <> $2
          AND status = 'completed'
          AND result_json IS NOT NULL
        ORDER BY finished_at DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(url)
    .bind(current_job_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, CRAWL_SCHEMA_LOCK_KEY).await?;

    {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_crawl_jobs (
                id UUID PRIMARY KEY,
                url TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('pending','running','completed','failed','canceled')),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                started_at TIMESTAMPTZ,
                finished_at TIMESTAMPTZ,
                error_text TEXT,
                result_json JSONB,
                config_json JSONB NOT NULL
            )
            "#,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_axon_crawl_jobs_status ON axon_crawl_jobs(status)",
        )
        .execute(&mut *tx)
        .await?;

        // Composite partial index for claim_next_pending: WHERE status='pending' ORDER BY created_at
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_axon_crawl_jobs_pending ON axon_crawl_jobs(created_at ASC) WHERE status = 'pending'"
        )
        .execute(&mut *tx)
        .await?;

        // Add CHECK constraint to existing tables (idempotent via IF NOT EXISTS pattern).
        sqlx::query(
            r#"DO $$ BEGIN
                ALTER TABLE axon_crawl_jobs ADD CONSTRAINT axon_crawl_jobs_status_check
                    CHECK (status IN ('pending','running','completed','failed','canceled'));
            EXCEPTION WHEN duplicate_object THEN NULL;
            END $$"#,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

// Public API delegates to db module.
pub use db::{
    cancel_job, cleanup_jobs, clear_jobs, doctor, get_job, list_jobs, recover_stale_crawl_jobs,
    reenqueue_orphaned_pending_jobs, start_crawl_job, start_crawl_jobs_batch,
};

pub async fn run_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    worker::run_worker(cfg).await
}

#[cfg(test)]
mod tests;

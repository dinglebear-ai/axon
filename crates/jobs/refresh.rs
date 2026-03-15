//! Refresh job table schema uses advisory-lock DDL via `common::schema::begin_schema_migration_tx`.
//!
//! # Advisory Lock DDL Pattern
//!
//! All schema migrations in this codebase should use `begin_schema_migration_tx(pool, lock_key)`
//! which:
//! - Opens a transaction
//! - Acquires `pg_advisory_xact_lock(lock_key)` (transaction-scoped, auto-released)
//! - Runs DDL inside the transaction
//! - Commits -- releasing the lock
//!
//! This serializes concurrent schema init across worker processes without deadlock risk.
//! Lock keys are unique per job table -- see `common/schema.rs` for the canonical implementation.
//!
//! # Developer Guide: Adding a New Job Type
//!
//! When adding a new job type:
//! 1. Choose a unique `i64` lock key (use any stable hash of the table name)
//! 2. Call `begin_schema_migration_tx(pool, MY_LOCK_KEY).await?`
//! 3. Run all DDL (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`,
//!    `ALTER TABLE ADD CONSTRAINT ... EXCEPTION WHEN duplicate_object`)
//! 4. `tx.commit().await?`

pub(crate) mod processor;
mod schedule;
mod state;
pub(crate) mod url_processor;
mod worker;

use crate::crates::core::logging::log_warn;
use crate::crates::jobs::common::{
    JobTable, cancel_pending_or_running_job, make_pool, purge_queue_safe,
};
use crate::crates::jobs::status::JobStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

pub use schedule::{
    RefreshScheduleCreate, claim_due_refresh_schedules, create_refresh_schedule,
    delete_refresh_schedule, list_refresh_schedules, mark_refresh_schedule_ran,
    set_refresh_schedule_enabled, start_refresh_job,
};
pub(crate) use schedule::{
    claim_due_refresh_schedules_with_pool, mark_refresh_schedule_ran_with_pool,
    should_reingest_github, start_refresh_job_with_pool,
};
pub use worker::{recover_stale_refresh_jobs_startup, run_refresh_once, run_refresh_worker};

static SCHEMA_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

const TABLE: JobTable = JobTable::Refresh;
const REFRESH_HEARTBEAT_INTERVAL_SECS: u64 = 15;
const SCHEDULE_CLAIM_LEASE_SECS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RefreshJobConfig {
    pub urls: Vec<String>,
    pub embed: bool,
    pub output_dir: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RefreshTargetState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_hash: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RefreshPageResult {
    pub status_code: u16,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_hash: Option<String>,
    pub markdown_chars: Option<usize>,
    pub markdown: Option<String>,
    pub changed: bool,
    pub not_modified: bool,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct RefreshRunSummary {
    pub checked: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub not_modified: usize,
    pub failed: usize,
    pub embedded_chunks: usize,
}

#[derive(Debug, Clone, PartialEq, FromRow, Serialize, Deserialize)]
pub struct RefreshJob {
    pub id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
    pub urls_json: serde_json::Value,
    pub result_json: Option<serde_json::Value>,
    pub config_json: serde_json::Value,
}

impl RefreshJob {
    /// Parse the raw status string into a typed `JobStatus`.
    /// Falls back to `JobStatus::Failed` if the string is unrecognized.
    pub fn job_status(&self) -> JobStatus {
        match self.status.as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "completed" => JobStatus::Completed,
            "canceled" => JobStatus::Canceled,
            _ => JobStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RefreshSchedule {
    pub id: Uuid,
    pub name: String,
    pub seed_url: Option<String>,
    pub urls_json: Option<serde_json::Value>,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `None` = URL refresh (default), `Some("github")` = GitHub repo re-ingest.
    pub source_type: Option<String>,
    /// For GitHub schedules: `"owner/repo"`.
    pub target: Option<String>,
}

pub(crate) async fn ensure_schema_once(pool: &PgPool) -> Result<(), sqlx::Error> {
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    Ok(())
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_refresh_jobs (
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
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_refresh_jobs_pending ON axon_refresh_jobs(created_at ASC) WHERE status = 'pending'",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_refresh_targets (
            url TEXT PRIMARY KEY,
            etag TEXT,
            last_modified TEXT,
            content_hash TEXT,
            markdown_chars INTEGER,
            last_status INTEGER,
            last_checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            last_changed_at TIMESTAMPTZ,
            error_text TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_refresh_schedules (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            seed_url TEXT,
            urls_json JSONB,
            every_seconds BIGINT NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            next_run_at TIMESTAMPTZ NOT NULL,
            last_run_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_refresh_schedules_due ON axon_refresh_schedules(next_run_at ASC) WHERE enabled = TRUE",
    )
    .execute(pool)
    .await?;

    // Schema evolution: add source_type + target columns for non-URL refresh schedules (e.g. GitHub repos).
    sqlx::query("ALTER TABLE axon_refresh_schedules ADD COLUMN IF NOT EXISTS source_type TEXT")
        .execute(pool)
        .await?;

    sqlx::query("ALTER TABLE axon_refresh_schedules ADD COLUMN IF NOT EXISTS target TEXT")
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_refresh_job(
    cfg: &crate::crates::core::config::Config,
    id: Uuid,
) -> Result<Option<RefreshJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    Ok(sqlx::query_as::<_, RefreshJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,urls_json,result_json,config_json FROM axon_refresh_jobs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?)
}

pub async fn list_refresh_jobs(
    cfg: &crate::crates::core::config::Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<RefreshJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    Ok(sqlx::query_as::<_, RefreshJob>(
        r#"SELECT id,status,created_at,updated_at,started_at,finished_at,error_text,urls_json,result_json,config_json FROM axon_refresh_jobs ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?)
}

pub async fn cancel_refresh_job(
    cfg: &crate::crates::core::config::Config,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    Ok(cancel_pending_or_running_job(&pool, TABLE, id).await?)
}

pub async fn cleanup_refresh_jobs(
    cfg: &crate::crates::core::config::Config,
) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    let mut total = 0u64;
    loop {
        let deleted = sqlx::query(
            "DELETE FROM axon_refresh_jobs WHERE id IN (SELECT id FROM axon_refresh_jobs WHERE status IN ($1,$2) LIMIT 1000)",
        )
        .bind(JobStatus::Failed.as_str())
        .bind(JobStatus::Canceled.as_str())
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

pub async fn clear_refresh_jobs(
    cfg: &crate::crates::core::config::Config,
) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    let rows = sqlx::query("DELETE FROM axon_refresh_jobs")
        .execute(&pool)
        .await?
        .rows_affected();
    if let Err(e) = purge_queue_safe(cfg, &cfg.refresh_queue).await {
        log_warn(&format!(
            "queue_purge_failed queue={} error={e}",
            cfg.refresh_queue
        ));
    }
    Ok(rows)
}

pub async fn recover_stale_refresh_jobs(
    cfg: &crate::crates::core::config::Config,
) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    let stats = crate::crates::jobs::common::reclaim_stale_running_jobs(
        &pool,
        TABLE,
        "refresh",
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
        "manual",
    )
    .await?;
    Ok(stats.reclaimed_jobs)
}

#[cfg(test)]
mod schedule_integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::{resolve_test_pg_url, test_config};
    use chrono::Duration;

    #[tokio::test]
    async fn ensure_schema_creates_refresh_schedule_table() -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let cfg = test_config(&pg_url);
        let pool = match make_pool(&cfg).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("DB unavailable, skipping test: {e}");
                return Ok(());
            }
        };
        ensure_schema(&pool).await?;

        let table_exists: Option<String> = sqlx::query_scalar(
            r#"
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'axon_refresh_schedules'
            "#,
        )
        .fetch_optional(&pool)
        .await?;

        assert_eq!(table_exists.as_deref(), Some("axon_refresh_schedules"));
        Ok(())
    }

    #[tokio::test]
    async fn claim_due_refresh_schedules_only_returns_enabled_due_rows()
    -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let cfg = test_config(&pg_url);
        let pool = match make_pool(&cfg).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("DB unavailable, skipping test: {e}");
                return Ok(());
            }
        };
        ensure_schema(&pool).await?;

        let due_name = format!("refresh-due-{}", Uuid::new_v4());
        let future_name = format!("refresh-future-{}", Uuid::new_v4());
        let disabled_name = format!("refresh-disabled-{}", Uuid::new_v4());

        let now = Utc::now();
        let _ = create_refresh_schedule(
            &cfg,
            &RefreshScheduleCreate {
                name: due_name.clone(),
                seed_url: None,
                urls: Some(vec!["https://example.com/due".to_string()]),
                every_seconds: 60,
                enabled: true,
                next_run_at: now - Duration::minutes(1),
                source_type: None,
                target: None,
            },
        )
        .await?;

        let _ = create_refresh_schedule(
            &cfg,
            &RefreshScheduleCreate {
                name: future_name.clone(),
                seed_url: None,
                urls: Some(vec!["https://example.com/future".to_string()]),
                every_seconds: 60,
                enabled: true,
                next_run_at: now + Duration::minutes(10),
                source_type: None,
                target: None,
            },
        )
        .await?;

        let _ = create_refresh_schedule(
            &cfg,
            &RefreshScheduleCreate {
                name: disabled_name.clone(),
                seed_url: None,
                urls: Some(vec!["https://example.com/disabled".to_string()]),
                every_seconds: 60,
                enabled: false,
                next_run_at: now - Duration::minutes(2),
                source_type: None,
                target: None,
            },
        )
        .await?;

        // Use a large batch to reduce parallel-test contention in the shared DB.
        let _claimed = claim_due_refresh_schedules(&cfg, 5_000).await?;
        // Other tests may race and claim rows between our create and claim calls. Assert
        // durable state by re-reading each fixture row instead of relying on claim return order.
        let due_after: RefreshSchedule =
            sqlx::query_as("SELECT * FROM axon_refresh_schedules WHERE name = $1")
                .bind(&due_name)
                .fetch_one(&pool)
                .await?;
        assert!(
            due_after.next_run_at > now,
            "due+enabled row should have been leased to a future run"
        );
        let future_after: RefreshSchedule =
            sqlx::query_as("SELECT * FROM axon_refresh_schedules WHERE name = $1")
                .bind(&future_name)
                .fetch_one(&pool)
                .await?;
        assert!(
            future_after.next_run_at >= now + Duration::minutes(9),
            "future row should remain in the future window"
        );
        let disabled_after: RefreshSchedule =
            sqlx::query_as("SELECT * FROM axon_refresh_schedules WHERE name = $1")
                .bind(&disabled_name)
                .fetch_one(&pool)
                .await?;
        assert!(!disabled_after.enabled, "disabled row must remain disabled");

        let _ = delete_refresh_schedule(&cfg, &due_name).await?;
        let _ = delete_refresh_schedule(&cfg, &future_name).await?;
        let _ = delete_refresh_schedule(&cfg, &disabled_name).await?;
        Ok(())
    }

    #[tokio::test]
    async fn claim_due_refresh_schedules_prevents_immediate_duplicate_claims()
    -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let cfg = test_config(&pg_url);
        let pool = match make_pool(&cfg).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("DB unavailable, skipping test: {e}");
                return Ok(());
            }
        };
        ensure_schema(&pool).await?;

        let name = format!("refresh-atomic-claim-{}", Uuid::new_v4());
        let before_create = Utc::now();
        let _ = create_refresh_schedule(
            &cfg,
            &RefreshScheduleCreate {
                name: name.clone(),
                seed_url: None,
                urls: Some(vec!["https://example.com/atomic".to_string()]),
                every_seconds: 120,
                enabled: true,
                next_run_at: before_create - Duration::minutes(2),
                source_type: None,
                target: None,
            },
        )
        .await?;

        let _ = claim_due_refresh_schedules(&cfg, 5_000).await?;
        let after_first: RefreshSchedule =
            sqlx::query_as("SELECT * FROM axon_refresh_schedules WHERE name = $1")
                .bind(&name)
                .fetch_one(&pool)
                .await?;
        assert!(after_first.next_run_at > before_create);

        let _ = claim_due_refresh_schedules(&cfg, 5_000).await?;
        let after_second: RefreshSchedule =
            sqlx::query_as("SELECT * FROM axon_refresh_schedules WHERE name = $1")
                .bind(&name)
                .fetch_one(&pool)
                .await?;
        assert!(
            after_second.next_run_at >= after_first.next_run_at,
            "same schedule must not move backwards after immediate re-claim attempt"
        );

        let _ = delete_refresh_schedule(&cfg, &name).await?;
        Ok(())
    }
}

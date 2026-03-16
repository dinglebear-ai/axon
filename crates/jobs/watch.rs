use crate::crates::core::config::Config;
use crate::crates::jobs::common::{begin_schema_migration_tx, make_pool};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

static SCHEMA_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
const WATCH_SCHEMA_LOCK_KEY: i64 = 0x7761_7463_6800_0000_i64;
const WATCH_CLAIM_LEASE_SECS: i64 = 300;
pub const WATCH_RUN_STATUS_RUNNING: &str = "running";
pub const WATCH_RUN_STATUS_COMPLETED: &str = "completed";
pub const WATCH_RUN_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WatchDef {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchDefCreate {
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WatchRun {
    pub id: Uuid,
    pub watch_id: Uuid,
    pub status: String,
    pub dispatched_job_id: Option<Uuid>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub(crate) async fn ensure_schema_once(pool: &PgPool) -> Result<(), sqlx::Error> {
    if SCHEMA_INIT.get().is_none() {
        ensure_schema(pool).await?;
        let _ = SCHEMA_INIT.set(());
    }
    Ok(())
}

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = begin_schema_migration_tx(pool, WATCH_SCHEMA_LOCK_KEY).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_watch_defs (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            task_type TEXT NOT NULL,
            task_payload JSONB NOT NULL,
            every_seconds BIGINT NOT NULL CHECK (every_seconds > 0),
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            next_run_at TIMESTAMPTZ NOT NULL,
            lease_expires_at TIMESTAMPTZ,
            last_run_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_watch_defs_due ON axon_watch_defs(next_run_at ASC) WHERE enabled = TRUE",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_watch_runs (
            id UUID PRIMARY KEY,
            watch_id UUID NOT NULL REFERENCES axon_watch_defs(id) ON DELETE CASCADE,
            status TEXT NOT NULL CHECK (status IN ('pending','running','completed','failed','canceled')),
            dispatched_job_id UUID,
            error_text TEXT,
            result_json JSONB,
            started_at TIMESTAMPTZ,
            finished_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_axon_watch_runs_watch_id ON axon_watch_runs(watch_id, created_at DESC)",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS axon_watch_run_artifacts (
            id BIGSERIAL PRIMARY KEY,
            watch_run_id UUID NOT NULL REFERENCES axon_watch_runs(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            path TEXT,
            payload JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    create_watch_def_with_pool(&pool, input).await
}

pub(crate) async fn create_watch_def_with_pool(
    pool: &PgPool,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    let id = Uuid::new_v4();
    Ok(sqlx::query_as::<_, WatchDef>(
        r#"
        INSERT INTO axon_watch_defs (
            id, name, task_type, task_payload, every_seconds, enabled, next_run_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, name, task_type, task_payload, every_seconds, enabled, next_run_at,
                  lease_expires_at, last_run_at, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(&input.name)
    .bind(&input.task_type)
    .bind(&input.task_payload)
    .bind(input.every_seconds)
    .bind(input.enabled)
    .bind(input.next_run_at)
    .fetch_one(pool)
    .await?)
}

#[must_use = "deletion errors are logged by the caller; ignoring this silently drops failures"]
pub(crate) async fn delete_watch_def_with_pool(
    pool: &PgPool,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    sqlx::query("DELETE FROM axon_watch_defs WHERE name = $1")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    list_watch_defs_with_pool(&pool, limit).await
}

pub(crate) async fn list_watch_defs_with_pool(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    Ok(sqlx::query_as::<_, WatchDef>(
        r#"
        SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at,
               lease_expires_at, last_run_at, created_at, updated_at
        FROM axon_watch_defs
        ORDER BY next_run_at ASC, created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn claim_due_watches(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    claim_due_watches_with_pool(&pool, limit).await
}

pub(crate) async fn claim_due_watches_with_pool(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let mut tx = pool.begin().await?;
    let claimed = sqlx::query_as::<_, WatchDef>(
        r#"
        WITH due AS (
            SELECT id
            FROM axon_watch_defs
            WHERE enabled = TRUE
              AND next_run_at <= NOW()
              AND (lease_expires_at IS NULL OR lease_expires_at <= NOW())
            ORDER BY next_run_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        ),
        claimed AS (
            UPDATE axon_watch_defs w
            SET lease_expires_at = NOW() + make_interval(secs => $2::double precision),
                updated_at = NOW()
            FROM due
            WHERE w.id = due.id
            RETURNING w.id, w.name, w.task_type, w.task_payload, w.every_seconds, w.enabled,
                      w.next_run_at, w.lease_expires_at, w.last_run_at, w.created_at, w.updated_at
        )
        SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at,
               lease_expires_at, last_run_at, created_at, updated_at
        FROM claimed
        ORDER BY next_run_at ASC, created_at ASC
        "#,
    )
    .bind(limit)
    .bind(WATCH_CLAIM_LEASE_SECS)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(claimed)
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    create_watch_run_with_pool(&pool, watch_id, dispatched_job_id).await
}

pub(crate) async fn create_watch_run_with_pool(
    pool: &PgPool,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let id = Uuid::new_v4();
    Ok(sqlx::query_as::<_, WatchRun>(
        r#"
        INSERT INTO axon_watch_runs (id, watch_id, status, dispatched_job_id, started_at)
        VALUES ($1, $2, 'running', $3, NOW())
        RETURNING id, watch_id, status, dispatched_job_id, error_text, result_json,
                  started_at, finished_at, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(watch_id)
    .bind(dispatched_job_id)
    .fetch_one(pool)
    .await?)
}

pub(crate) async fn mark_watch_run_finished_with_pool(
    pool: &PgPool,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let mut tx = pool.begin().await?;
    let run_updated = sqlx::query(
        r#"
        UPDATE axon_watch_runs
        SET status = $3, result_json = $4, error_text = $5, finished_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND watch_id = $2
        "#,
    )
    .bind(run_id)
    .bind(watch_id)
    .bind(status)
    .bind(result_json)
    .bind(error_text)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if run_updated == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        r#"
        UPDATE axon_watch_defs
        SET last_run_at = NOW(),
            next_run_at = NOW() + make_interval(secs => every_seconds::double precision),
            lease_expires_at = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(watch_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema_once(&pool).await?;
    Ok(sqlx::query_as::<_, WatchRun>(
        r#"
        SELECT id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at
        FROM axon_watch_runs
        WHERE watch_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(watch_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::resolve_test_pg_url;

    #[tokio::test]
    async fn create_watch_persists_definition() -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let cfg = Config::test_default();
        let pool = match PgPool::connect(&pg_url).await {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        ensure_schema_once(&pool).await?;

        let name = format!("watch-create-{}", Uuid::new_v4());
        let created = create_watch_def_with_pool(
            &pool,
            &WatchDefCreate {
                name: name.clone(),
                task_type: "refresh".to_string(),
                task_payload: serde_json::json!({"urls":["https://example.com"]}),
                every_seconds: 60,
                enabled: true,
                next_run_at: Utc::now(),
            },
        )
        .await?;
        assert_eq!(created.name, name);
        assert_eq!(created.task_type, "refresh");
        let listed = list_watch_defs_with_pool(&pool, 100).await?;
        assert!(listed.iter().any(|w| w.id == created.id));

        let _ = sqlx::query("DELETE FROM axon_watch_defs WHERE id=$1")
            .bind(created.id)
            .execute(&pool)
            .await?;
        let _ = cfg;
        Ok(())
    }

    #[tokio::test]
    async fn claim_due_watches_uses_skip_locked_and_lease() -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let pool = match PgPool::connect(&pg_url).await {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        ensure_schema_once(&pool).await?;

        let created = create_watch_def_with_pool(
            &pool,
            &WatchDefCreate {
                name: format!("watch-claim-{}", Uuid::new_v4()),
                task_type: "refresh".to_string(),
                task_payload: serde_json::json!({}),
                every_seconds: 60,
                enabled: true,
                next_run_at: Utc::now() - chrono::Duration::minutes(1),
            },
        )
        .await?;

        let first = claim_due_watches_with_pool(&pool, 50).await?;
        assert!(first.iter().any(|w| w.id == created.id));
        let second = claim_due_watches_with_pool(&pool, 50).await?;
        assert!(!second.iter().any(|w| w.id == created.id));

        let _ = sqlx::query("DELETE FROM axon_watch_defs WHERE id=$1")
            .bind(created.id)
            .execute(&pool)
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn create_watch_run_records_dispatched_job() -> Result<(), Box<dyn Error>> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let pool = match PgPool::connect(&pg_url).await {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        ensure_schema_once(&pool).await?;

        let created = create_watch_def_with_pool(
            &pool,
            &WatchDefCreate {
                name: format!("watch-run-{}", Uuid::new_v4()),
                task_type: "refresh".to_string(),
                task_payload: serde_json::json!({}),
                every_seconds: 60,
                enabled: true,
                next_run_at: Utc::now(),
            },
        )
        .await?;
        let dispatched_job_id = Uuid::new_v4();
        let run = create_watch_run_with_pool(&pool, created.id, Some(dispatched_job_id)).await?;
        assert_eq!(run.watch_id, created.id);
        assert_eq!(run.dispatched_job_id, Some(dispatched_job_id));
        assert_eq!(run.status, "running");

        let _ = sqlx::query("DELETE FROM axon_watch_defs WHERE id=$1")
            .bind(created.id)
            .execute(&pool)
            .await?;
        Ok(())
    }
}

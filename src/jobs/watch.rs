use crate::core::config::Config;
use crate::jobs::store::{now_ms, open_config_pool};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::error::Error;
use uuid::Uuid;

pub(crate) mod change_detect;
pub(crate) mod cluster;
pub(crate) mod dispatch;
pub(crate) mod filter;
pub(crate) mod orchestrate;
pub(crate) mod report;
mod rows;
pub(crate) mod run_now;
mod validation;
use rows::{
    WatchDefRow, WatchRunArtifactRow, WatchRunRow, normalize_watch_def_create, parse_watch_def_row,
    parse_watch_run_artifact_row, parse_watch_run_row,
};
pub(crate) use run_now::run_leased_watch_now_with_pool;
pub use run_now::{run_watch_now, run_watch_now_with_pool};
use validation::{DEFAULT_WATCH_LEASE_SECS, MAX_WATCH_LIST_LIMIT};
pub use validation::{
    MAX_WATCH_DEPTH, MAX_WATCH_INTERVAL_SECS, MAX_WATCH_URLS, MIN_WATCH_INTERVAL_SECS,
    SUPPORTED_TASK_TYPES, WATCH_RUN_STATUS_COMPLETED, WATCH_RUN_STATUS_FAILED,
    WATCH_RUN_STATUS_RUNNING, validate_every_seconds, validate_task_payload, validate_task_type,
    validate_watch_def_create,
};
pub(crate) mod url_state;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(deny_unknown_fields)]
pub struct WatchDefCreate {
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchDefCreateRequest {
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: Option<bool>,
    pub next_run_at: Option<DateTime<Utc>>,
}

impl WatchDefCreateRequest {
    pub fn into_create(self) -> Result<WatchDefCreate, String> {
        validate_every_seconds(self.every_seconds)?;
        let input = WatchDefCreate {
            name: self.name.trim().to_string(),
            task_type: self.task_type,
            task_payload: self.task_payload,
            every_seconds: self.every_seconds,
            enabled: self.enabled.unwrap_or(true),
            next_run_at: self
                .next_run_at
                .unwrap_or_else(|| Utc::now() + Duration::seconds(self.every_seconds)),
        };
        validate_watch_def_create(&input)?;
        Ok(input)
    }
}

pub(crate) fn parse_watch_lease_secs(raw: Option<String>) -> i64 {
    raw.and_then(|raw| raw.parse::<i64>().ok())
        .filter(|secs| *secs >= 1)
        .unwrap_or(DEFAULT_WATCH_LEASE_SECS)
}

pub(crate) fn watch_lease_ttl_ms_from_env() -> i64 {
    parse_watch_lease_secs(std::env::var("AXON_WATCH_LEASE_SECS").ok()) * 1_000
}

fn clamp_watch_list_limit(limit: i64) -> i64 {
    limit.clamp(1, MAX_WATCH_LIST_LIMIT)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRunArtifact {
    pub id: i64,
    pub watch_run_id: Uuid,
    pub kind: String,
    pub path: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    create_watch_def_with_pool(&pool, input).await
}

pub async fn create_watch_def_with_pool(
    pool: &SqlitePool,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    let input = normalize_watch_def_create(input);
    validate_watch_def_create(&input).map_err(|msg| format!("watch create: {msg}"))?;
    let id = Uuid::new_v4();
    let now = now_ms();
    let row = sqlx::query_as::<_, WatchDefRow>(
        "INSERT INTO axon_watch_defs \
         (id, name, task_type, task_payload, every_seconds, enabled, next_run_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(&input.name)
    .bind(&input.task_type)
    .bind(input.task_payload.to_string())
    .bind(input.every_seconds)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(input.next_run_at.timestamp_millis())
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(parse_watch_def_row(row))
}

pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    list_watch_defs_with_pool(&pool, limit).await
}

pub async fn list_watch_defs_with_pool(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let rows = sqlx::query_as::<_, WatchDefRow>(
        "SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_watch_defs ORDER BY next_run_at ASC, created_at ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_def_row).collect())
}

/// Atomically lease every enabled watch that is due to run.
///
/// A single `UPDATE ... RETURNING` stamps `lease_expires_at = now + lease_ttl_ms`
/// onto each enabled row with `next_run_at <= now` and a free lease (NULL or
/// expired), returning the leased defs. SQLite serializes writers and the
/// statement is atomic, so two schedulers can never lease the same watch twice.
///
/// The same statement also advances `next_run_at` to `now + every_seconds` at
/// lease time. This is the single-flight guarantee: even if a run outlives its
/// `lease_expires_at` TTL (e.g. a refresh task with many slow URLs running past
/// `AXON_WATCH_LEASE_SECS`), the row is no longer due, so a later sweep cannot
/// re-lease and double-fire it while the first run is still in flight.
/// `finish_watch_run_with_pool` re-stamps `next_run_at` from the completion time
/// and clears the lease. Tradeoff: a crashed run is retried at the next interval
/// (once `reclaim_stale_watch_leases` frees the lease) rather than immediately.
pub async fn lease_due_watches(
    pool: &SqlitePool,
    now: i64,
    lease_ttl_ms: i64,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    let lease_until = now + lease_ttl_ms;
    let rows = sqlx::query_as::<_, WatchDefRow>(
        "UPDATE axon_watch_defs \
         SET lease_expires_at = ?, next_run_at = ? + (every_seconds * 1000), updated_at = ? \
         WHERE id IN ( \
             SELECT id FROM axon_watch_defs \
             WHERE enabled = 1 AND next_run_at <= ? \
               AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
             ORDER BY next_run_at ASC LIMIT ? \
         ) \
         RETURNING id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(lease_until)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_def_row).collect())
}

/// Acquire a lease for an explicit/manual run-now call.
///
/// Unlike [`lease_due_watches`], this does not require `next_run_at <= now`;
/// run-now is intentionally immediate. It still enforces the same active-lease
/// single-flight guard and advances `next_run_at` so a scheduler tick cannot
/// pick up the same watch while the manual run is in flight.
pub(super) async fn lease_watch_for_manual_run(
    pool: &SqlitePool,
    watch_id: Uuid,
    now: i64,
    lease_ttl_ms: i64,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    let lease_until = now + lease_ttl_ms;
    let row = sqlx::query_as::<_, WatchDefRow>(
        "UPDATE axon_watch_defs \
         SET lease_expires_at = ?, next_run_at = ? + (every_seconds * 1000), updated_at = ? \
         WHERE id = ? AND enabled = 1 \
           AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
         RETURNING id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(lease_until)
    .bind(now)
    .bind(now)
    .bind(watch_id.to_string())
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_watch_def_row))
}

pub async fn get_watch_def(
    cfg: &Config,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    get_watch_def_with_pool(&pool, watch_id).await
}

pub async fn get_watch_def_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
) -> Result<Option<WatchDef>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, WatchDefRow>(
        "SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_watch_defs WHERE id = ?",
    )
    .bind(watch_id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_watch_def_row))
}

pub async fn create_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    create_watch_run_with_pool(&pool, watch_id, dispatched_job_id).await
}

pub async fn create_watch_run_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<WatchRun, Box<dyn Error>> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let row = sqlx::query_as::<_, WatchRunRow>(
        "INSERT INTO axon_watch_runs \
         (id, watch_id, status, dispatched_job_id, started_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(watch_id.to_string())
    .bind(WATCH_RUN_STATUS_RUNNING)
    .bind(dispatched_job_id.map(|value| value.to_string()))
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(parse_watch_run_row(row))
}

pub async fn finish_watch_run(
    cfg: &Config,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    finish_watch_run_with_pool(&pool, watch_id, run_id, status, result_json, error_text).await
}

pub async fn finish_watch_run_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let now = now_ms();
    let updated = sqlx::query(
        "UPDATE axon_watch_runs \
         SET status = ?, result_json = ?, error_text = ?, finished_at = ?, updated_at = ? \
         WHERE id = ? AND watch_id = ?",
    )
    .bind(status)
    .bind(result_json.map(serde_json::Value::to_string))
    .bind(error_text)
    .bind(now)
    .bind(now)
    .bind(run_id.to_string())
    .bind(watch_id.to_string())
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE axon_watch_defs \
         SET last_run_at = ?, next_run_at = ? + (every_seconds * 1000), lease_expires_at = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(watch_id.to_string())
    .execute(pool)
    .await?;

    Ok(true)
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    list_watch_runs_with_pool(&pool, watch_id, limit).await
}

pub async fn list_watch_runs_with_pool(
    pool: &SqlitePool,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    let limit = clamp_watch_list_limit(limit);
    let rows = sqlx::query_as::<_, WatchRunRow>(
        "SELECT id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at \
         FROM axon_watch_runs WHERE watch_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(watch_id.to_string())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_run_row).collect())
}

pub async fn list_watch_run_artifacts(
    cfg: &Config,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    list_watch_run_artifacts_with_pool(&pool, run_id, limit).await
}

pub async fn list_watch_run_artifacts_with_pool(
    pool: &SqlitePool,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRunArtifact>, Box<dyn Error>> {
    let limit = clamp_watch_list_limit(limit);
    let rows = sqlx::query_as::<_, WatchRunArtifactRow>(
        "SELECT id, watch_run_id, kind, path, payload, created_at \
         FROM axon_watch_run_artifacts WHERE watch_run_id = ? \
         ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(run_id.to_string())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_watch_run_artifact_row).collect())
}

pub(super) async fn get_watch_run_with_pool(
    pool: &SqlitePool,
    run_id: Uuid,
) -> Result<Option<WatchRun>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, WatchRunRow>(
        "SELECT id, watch_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, created_at, updated_at \
         FROM axon_watch_runs WHERE id = ? LIMIT 1",
    )
    .bind(run_id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_watch_run_row))
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

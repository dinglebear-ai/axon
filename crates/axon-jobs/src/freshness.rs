use crate::freshness::rows::{
    FreshnessDefRow, FreshnessRunRow, normalize_freshness_def_create, parse_freshness_def_row,
    parse_freshness_run_row,
};
use crate::store::{now_ms, open_config_pool};
use axon_core::config::Config;
use axon_core::redact::redact_secrets;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::error::Error;
use uuid::Uuid;

mod rows;

pub const MIN_FRESHNESS_INTERVAL_SECS: i64 = 86_400;
pub const MAX_FRESHNESS_INTERVAL_SECS: i64 = 366 * 86_400;
pub const MAX_FRESHNESS_DEFS: i64 = 10_000;
pub const MAX_FRESHNESS_LIST_LIMIT: i64 = 1_000;
pub const FRESHNESS_RUN_STATUS_RUNNING: &str = "running";
pub const FRESHNESS_RUN_STATUS_COMPLETED: &str = "completed";
pub const FRESHNESS_RUN_STATUS_FAILED: &str = "failed";
pub const FRESHNESS_RUN_STATUS_ENQUEUED: &str = "enqueued";
pub const FRESHNESS_RUN_STATUS_SKIPPED_ACTIVE_JOB: &str = "skipped_active_job";

const MAX_ERROR_TEXT_CHARS: usize = 4096;
const MAX_RESULT_JSON_CHARS: usize = 65_536;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessDef {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub target: String,
    pub identity_hash: String,
    pub request_json: serde_json::Value,
    pub config_json: serde_json::Value,
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
pub struct FreshnessDefCreate {
    pub name: String,
    pub command: String,
    pub target: String,
    pub identity_hash: String,
    pub request_json: serde_json::Value,
    pub config_json: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreshnessRun {
    pub id: Uuid,
    pub freshness_id: Uuid,
    pub status: String,
    pub dispatched_job_id: Option<Uuid>,
    pub error_text: Option<String>,
    pub result_json: Option<serde_json::Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn stable_initial_jitter_seconds(identity_hash: &str, every_seconds: i64) -> i64 {
    let max = std::cmp::min(3600, every_seconds / 10).max(1);
    let prefix_raw = identity_hash.get(..16).unwrap_or(identity_hash);
    let prefix = u64::from_str_radix(prefix_raw, 16).unwrap_or(0);
    (prefix % max as u64) as i64
}

fn clamp_freshness_list_limit(limit: i64) -> i64 {
    limit.clamp(1, MAX_FRESHNESS_LIST_LIMIT)
}

fn validate_freshness_def_create(input: &FreshnessDefCreate) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("name must not be empty".to_string());
    }
    match input.command.as_str() {
        "scrape" | "crawl" | "embed" | "ingest" => {}
        other => return Err(format!("unsupported freshness command {other:?}")),
    }
    if input.target.trim().is_empty() {
        return Err("target must not be empty".to_string());
    }
    if input.identity_hash.trim().is_empty() {
        return Err("identity_hash must not be empty".to_string());
    }
    if !(MIN_FRESHNESS_INTERVAL_SECS..=MAX_FRESHNESS_INTERVAL_SECS).contains(&input.every_seconds) {
        return Err(format!(
            "every_seconds must be between {MIN_FRESHNESS_INTERVAL_SECS} and {MAX_FRESHNESS_INTERVAL_SECS}"
        ));
    }
    if !input.request_json.is_object() {
        return Err("request_json must be an object".to_string());
    }
    if !input.config_json.is_object() {
        return Err("config_json must be an object".to_string());
    }
    Ok(())
}

fn next_run_ms_for_create(input: &FreshnessDefCreate, now: i64) -> i64 {
    input.next_run_at.map_or_else(
        || {
            now + input.every_seconds * 1_000
                + stable_initial_jitter_seconds(&input.identity_hash, input.every_seconds) * 1_000
        },
        |next_run| next_run.timestamp_millis(),
    )
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn sanitize_error_text(error_text: Option<&str>) -> Option<String> {
    error_text.map(|text| truncate_chars(&redact_secrets(text), MAX_ERROR_TEXT_CHARS))
}

fn sanitize_result_json(result_json: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let value = result_json?;
    let redacted = redact_secrets(&value.to_string());
    if redacted.chars().count() > MAX_RESULT_JSON_CHARS {
        return Some(serde_json::json!({
            "truncated": true,
            "text": truncate_chars(&redacted, MAX_RESULT_JSON_CHARS),
        }));
    }
    serde_json::from_str(&redacted).ok().or_else(|| {
        Some(serde_json::json!({
            "text": redacted,
        }))
    })
}

pub async fn create_freshness_def(
    cfg: &Config,
    input: &FreshnessDefCreate,
) -> Result<FreshnessDef, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    create_freshness_def_with_pool(&pool, input).await
}

pub async fn create_freshness_def_with_pool(
    pool: &SqlitePool,
    input: &FreshnessDefCreate,
) -> Result<FreshnessDef, Box<dyn Error>> {
    let input = normalize_freshness_def_create(input);
    validate_freshness_def_create(&input).map_err(|msg| format!("freshness create: {msg}"))?;
    let current_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_freshness_defs")
        .fetch_one(pool)
        .await?;
    let id = Uuid::new_v4();
    let now = now_ms();
    let next_run_at = next_run_ms_for_create(&input, now);
    let row = sqlx::query_as::<_, FreshnessDefRow>(
        "INSERT INTO axon_freshness_defs \
         (id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(identity_hash) DO UPDATE SET \
           name = excluded.name, command = excluded.command, target = excluded.target, \
           request_json = excluded.request_json, config_json = excluded.config_json, \
           every_seconds = excluded.every_seconds, enabled = excluded.enabled, \
           next_run_at = excluded.next_run_at, lease_expires_at = NULL, updated_at = excluded.updated_at \
         RETURNING id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(&input.name)
    .bind(&input.command)
    .bind(&input.target)
    .bind(&input.identity_hash)
    .bind(input.request_json.to_string())
    .bind(input.config_json.to_string())
    .bind(input.every_seconds)
    .bind(if input.enabled { 1 } else { 0 })
    .bind(next_run_at)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if current_count >= MAX_FRESHNESS_DEFS {
            format!("freshness create: maximum freshness schedule count {MAX_FRESHNESS_DEFS} reached").into()
        } else {
            Box::<dyn Error>::from(err)
        }
    })?;
    Ok(parse_freshness_def_row(row))
}

pub async fn list_freshness_defs(
    cfg: &Config,
    limit: i64,
) -> Result<Vec<FreshnessDef>, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    list_freshness_defs_with_pool(&pool, limit).await
}

pub async fn list_freshness_defs_with_pool(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<FreshnessDef>, Box<dyn Error>> {
    let rows = sqlx::query_as::<_, FreshnessDefRow>(
        "SELECT id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_freshness_defs ORDER BY next_run_at ASC, created_at ASC LIMIT ?",
    )
    .bind(clamp_freshness_list_limit(limit))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_freshness_def_row).collect())
}

pub async fn get_freshness_def_with_pool(
    pool: &SqlitePool,
    freshness_id: Uuid,
) -> Result<Option<FreshnessDef>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, FreshnessDefRow>(
        "SELECT id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at \
         FROM axon_freshness_defs WHERE id = ?",
    )
    .bind(freshness_id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_freshness_def_row))
}

pub async fn lease_due_freshness(
    pool: &SqlitePool,
    now: i64,
    lease_ttl_ms: i64,
    limit: i64,
) -> Result<Vec<FreshnessDef>, Box<dyn Error>> {
    let lease_until = now + lease_ttl_ms;
    let rows = sqlx::query_as::<_, FreshnessDefRow>(
        "UPDATE axon_freshness_defs \
         SET lease_expires_at = ?, next_run_at = ? + (every_seconds * 1000), updated_at = ? \
         WHERE id IN ( \
             SELECT id FROM axon_freshness_defs \
             WHERE enabled = 1 AND next_run_at <= ? \
               AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
             ORDER BY next_run_at ASC LIMIT ? \
         ) \
         RETURNING id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(lease_until)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(limit.clamp(1, 4))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_freshness_def_row).collect())
}

pub async fn lease_freshness_for_manual_run(
    pool: &SqlitePool,
    freshness_id: Uuid,
    now: i64,
    lease_ttl_ms: i64,
) -> Result<Option<FreshnessDef>, Box<dyn Error>> {
    let lease_until = now + lease_ttl_ms;
    let row = sqlx::query_as::<_, FreshnessDefRow>(
        "UPDATE axon_freshness_defs \
         SET lease_expires_at = ?, next_run_at = ? + (every_seconds * 1000), updated_at = ? \
         WHERE id = ? AND enabled = 1 \
           AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
         RETURNING id, name, command, target, identity_hash, request_json, config_json, every_seconds, enabled, next_run_at, lease_expires_at, last_run_at, created_at, updated_at",
    )
    .bind(lease_until)
    .bind(now)
    .bind(now)
    .bind(freshness_id.to_string())
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(parse_freshness_def_row))
}

pub async fn create_freshness_run_with_pool(
    pool: &SqlitePool,
    freshness_id: Uuid,
    dispatched_job_id: Option<Uuid>,
) -> Result<FreshnessRun, Box<dyn Error>> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let row = sqlx::query_as::<_, FreshnessRunRow>(
        "INSERT INTO axon_freshness_runs \
         (id, freshness_id, status, dispatched_job_id, started_at, heartbeat_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, freshness_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, heartbeat_at, created_at, updated_at",
    )
    .bind(id.to_string())
    .bind(freshness_id.to_string())
    .bind(FRESHNESS_RUN_STATUS_RUNNING)
    .bind(dispatched_job_id.map(|value| value.to_string()))
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(parse_freshness_run_row(row))
}

pub async fn heartbeat_freshness_run(
    pool: &SqlitePool,
    freshness_id: Uuid,
    run_id: Uuid,
    lease_expires_at_ms: i64,
) -> Result<bool, Box<dyn Error>> {
    let now = now_ms();
    let run_updated = sqlx::query(
        "UPDATE axon_freshness_runs SET heartbeat_at = ?, updated_at = ? \
         WHERE id = ? AND freshness_id = ? AND status = ?",
    )
    .bind(now)
    .bind(now)
    .bind(run_id.to_string())
    .bind(freshness_id.to_string())
    .bind(FRESHNESS_RUN_STATUS_RUNNING)
    .execute(pool)
    .await?
    .rows_affected();
    if run_updated == 0 {
        return Ok(false);
    }

    sqlx::query("UPDATE axon_freshness_defs SET lease_expires_at = ?, updated_at = ? WHERE id = ?")
        .bind(lease_expires_at_ms)
        .bind(now)
        .bind(freshness_id.to_string())
        .execute(pool)
        .await?;
    Ok(true)
}

pub async fn set_freshness_run_dispatched_job_with_pool(
    pool: &SqlitePool,
    freshness_id: Uuid,
    run_id: Uuid,
    dispatched_job_id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    let now = now_ms();
    let updated = sqlx::query(
        "UPDATE axon_freshness_runs SET dispatched_job_id = ?, updated_at = ? \
         WHERE id = ? AND freshness_id = ?",
    )
    .bind(dispatched_job_id.to_string())
    .bind(now)
    .bind(run_id.to_string())
    .bind(freshness_id.to_string())
    .execute(pool)
    .await?
    .rows_affected();
    Ok(updated > 0)
}

pub async fn finish_freshness_run_with_pool(
    pool: &SqlitePool,
    freshness_id: Uuid,
    run_id: Uuid,
    status: &str,
    result_json: Option<&serde_json::Value>,
    error_text: Option<&str>,
) -> Result<bool, Box<dyn Error>> {
    let now = now_ms();
    let sanitized_result = sanitize_result_json(result_json);
    let sanitized_error = sanitize_error_text(error_text);
    let updated = sqlx::query(
        "UPDATE axon_freshness_runs \
         SET status = ?, result_json = ?, error_text = ?, finished_at = ?, updated_at = ? \
         WHERE id = ? AND freshness_id = ?",
    )
    .bind(status)
    .bind(sanitized_result.map(|value| value.to_string()))
    .bind(sanitized_error)
    .bind(now)
    .bind(now)
    .bind(run_id.to_string())
    .bind(freshness_id.to_string())
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE axon_freshness_defs \
         SET last_run_at = ?, next_run_at = ? + (every_seconds * 1000), lease_expires_at = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(freshness_id.to_string())
    .execute(pool)
    .await?;

    Ok(true)
}

pub async fn list_freshness_runs_with_pool(
    pool: &SqlitePool,
    freshness_id: Uuid,
    limit: i64,
) -> Result<Vec<FreshnessRun>, Box<dyn Error>> {
    let rows = sqlx::query_as::<_, FreshnessRunRow>(
        "SELECT id, freshness_id, status, dispatched_job_id, error_text, result_json, started_at, finished_at, heartbeat_at, created_at, updated_at \
         FROM axon_freshness_runs WHERE freshness_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(freshness_id.to_string())
    .bind(clamp_freshness_list_limit(limit))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(parse_freshness_run_row).collect())
}

pub async fn reclaim_stale_freshness_leases(
    pool: &SqlitePool,
    now: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE axon_freshness_defs SET lease_expires_at = NULL WHERE lease_expires_at < ?",
    )
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn reclaim_current_stale_freshness_leases(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    reclaim_stale_freshness_leases(pool, now_ms()).await
}

pub fn freshness_lease_ttl_ms_from_secs(lease_secs: i64) -> i64 {
    lease_secs.max(1) * 1_000
}

pub fn freshness_duration_seconds(days: i64) -> i64 {
    Duration::days(days).num_seconds()
}

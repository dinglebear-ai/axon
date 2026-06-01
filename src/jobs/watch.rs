use crate::core::config::Config;
use crate::jobs::query::ms_to_dt;
use crate::jobs::store::{now_ms, open_config_pool};
use chrono::{DateTime, Utc};
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
pub(crate) mod url_state;

pub const WATCH_RUN_STATUS_RUNNING: &str = "running";
pub const WATCH_RUN_STATUS_COMPLETED: &str = "completed";
pub const WATCH_RUN_STATUS_FAILED: &str = "failed";

/// Task types a watch may carry. A `task_type` outside this set can never run,
/// so every create path (CLI, HTTP) must validate against this single list.
pub const SUPPORTED_TASK_TYPES: &[&str] = &["watch"];

/// Validate a `task_type` at create time so callers never persist a watch that
/// can never execute. Rejects surrounding whitespace (the stored value would
/// otherwise fail the exact-match dispatch) and any type outside
/// [`SUPPORTED_TASK_TYPES`]. The message is safe for entry points to surface.
pub fn validate_task_type(task_type: &str) -> Result<(), String> {
    if task_type != task_type.trim() {
        return Err("task_type must not have leading or trailing whitespace".to_string());
    }
    if !SUPPORTED_TASK_TYPES.contains(&task_type) {
        return Err(format!(
            "unsupported task_type: '{}'; supported: {}",
            task_type,
            SUPPORTED_TASK_TYPES.join(", ")
        ));
    }
    Ok(())
}

/// Maximum number of URLs a single watch may track. Bounds per-tick work and
/// the persisted payload.
pub const MAX_WATCH_URLS: usize = 256;
/// Maximum crawl depth a watch payload may request for change-triggered crawls.
pub const MAX_WATCH_DEPTH: u64 = 10;

/// Validate a watch's `task_payload` at create time: `urls` non-empty (and
/// within [`MAX_WATCH_URLS`]), every `ignore_patterns` entry compiles as a
/// regex, and any `max_depth` is within [`MAX_WATCH_DEPTH`]. Shared by CLI +
/// HTTP create.
pub fn validate_task_payload(payload: &serde_json::Value) -> Result<(), String> {
    let urls = payload
        .get("urls")
        .and_then(|v| v.as_array())
        .ok_or("task_payload.urls must be a non-empty array")?;
    if urls.is_empty() || !urls.iter().all(|u| u.is_string()) {
        return Err("task_payload.urls must be a non-empty array of strings".to_string());
    }
    if urls.len() > MAX_WATCH_URLS {
        return Err(format!(
            "task_payload.urls has {} entries; maximum is {MAX_WATCH_URLS}",
            urls.len()
        ));
    }
    if let Some(depth) = payload.get("max_depth") {
        let n = depth
            .as_u64()
            .ok_or("task_payload.max_depth must be a non-negative integer")?;
        if n > MAX_WATCH_DEPTH {
            return Err(format!(
                "task_payload.max_depth is {n}; maximum is {MAX_WATCH_DEPTH}"
            ));
        }
    }
    if let Some(pats) = payload.get("ignore_patterns") {
        let arr = pats
            .as_array()
            .ok_or("ignore_patterns must be an array of strings")?;
        for p in arr {
            let s = p
                .as_str()
                .ok_or("ignore_patterns entries must be strings")?;
            regex::Regex::new(s).map_err(|e| format!("invalid ignore_pattern '{s}': {e}"))?;
        }
    }
    Ok(())
}

/// Minimum allowed watch interval. The scheduler leases purely on
/// `next_run_at <= now`, so a sub-minimum interval would auto-fire too often.
pub const MIN_WATCH_INTERVAL_SECS: i64 = 30;
/// Maximum allowed watch interval (7 days).
pub const MAX_WATCH_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60;

/// Validate `every_seconds` at create time. Centralized (like
/// [`validate_task_type`]) so every create path — REST `/v1/watch/create`,
/// admin `/v1/watch`, and the CLI — enforces identical bounds and the
/// scheduler can never lease a sub-minimum watch. The message is safe to
/// surface to callers.
pub fn validate_every_seconds(every_seconds: i64) -> Result<(), String> {
    if !(MIN_WATCH_INTERVAL_SECS..=MAX_WATCH_INTERVAL_SECS).contains(&every_seconds) {
        return Err(format!(
            "every_seconds must be between {MIN_WATCH_INTERVAL_SECS} and {MAX_WATCH_INTERVAL_SECS}"
        ));
    }
    Ok(())
}

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

type WatchDefRow = (
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

type WatchRunRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

fn parse_watch_def_row(row: WatchDefRow) -> WatchDef {
    let (
        id,
        name,
        task_type,
        task_payload,
        every_seconds,
        enabled,
        next_run_at,
        lease_expires_at,
        last_run_at,
        created_at,
        updated_at,
    ) = row;
    WatchDef {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        name,
        task_type,
        task_payload: parse_json(&task_payload),
        every_seconds,
        enabled: enabled != 0,
        next_run_at: ms_to_dt(next_run_at),
        lease_expires_at: lease_expires_at.map(ms_to_dt),
        last_run_at: last_run_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

fn parse_watch_run_row(row: WatchRunRow) -> WatchRun {
    let (
        id,
        watch_id,
        status,
        dispatched_job_id,
        error_text,
        result_json,
        started_at,
        finished_at,
        created_at,
        updated_at,
    ) = row;
    WatchRun {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        watch_id: Uuid::parse_str(&watch_id).unwrap_or_default(),
        status,
        dispatched_job_id: dispatched_job_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
        error_text,
        result_json: result_json.as_deref().map(parse_json),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
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

async fn get_watch_run_with_pool(
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

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    run_watch_now_with_pool(cfg, &pool, watch).await
}

pub async fn run_watch_now_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
    watch: &WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    let run = create_watch_run_with_pool(pool, watch.id, None).await?;

    // Execute first (no DB writes), then finalize exactly once. `err_text` is a
    // `String`, not a boxed `dyn Error`, so the box never crosses an await and
    // the future stays `Send` for the axum runtime behind `/v1/watch/{id}/run`.
    // A COMPLETED write that fails falls through to the FAILED finalize below so
    // the run is never wedged in `running` — nothing reclaims stale runs.
    let outcome: Result<serde_json::Value, String> = run_watch_task(cfg, pool, run.id, watch).await;
    let err_text = match outcome {
        Ok(payload) => match finalize_completed(pool, watch, run.id, &payload).await {
            Ok(()) => return Ok(get_watch_run_with_pool(pool, run.id).await?.unwrap_or(run)),
            Err(text) => text,
        },
        Err(text) => text,
    };
    if let Err(persist_err) = finish_watch_run_with_pool(
        pool,
        watch.id,
        run.id,
        WATCH_RUN_STATUS_FAILED,
        None,
        Some(&err_text),
    )
    .await
    {
        // The FAILED-status write itself failed: the run row stays in `running`
        // and nothing reclaims stale watch runs, so it is wedged permanently.
        // Surface why instead of dropping the error silently.
        tracing::warn!(
            watch_id = %watch.id,
            run_id = %run.id,
            persist_error = %persist_err,
            task_error = %err_text,
            "watch run: FAILED-status write failed; run may be wedged in running",
        );
    }
    Err(err_text.into())
}

/// Persist a COMPLETED run, mapping any error to a `String` so the non-`Send` box is dropped before the caller's next await.
async fn finalize_completed(
    pool: &SqlitePool,
    watch: &WatchDef,
    run_id: Uuid,
    payload: &serde_json::Value,
) -> Result<(), String> {
    finish_watch_run_with_pool(
        pool,
        watch.id,
        run_id,
        WATCH_RUN_STATUS_COMPLETED,
        Some(payload),
        None,
    )
    .await
    .map(|_| ())
    .map_err(|err| err.to_string())
}

/// Execute a watch's task → result payload, or a human-readable failure message.
/// Pure compute + scrape; the caller owns the single finalize write. Receives
/// the caller's `pool` and the real `run_id` so the orchestrator never has to
/// re-derive the current run (which was racy when a `run-now` overlapped a
/// scheduled run) or open a fresh per-run pool.
async fn run_watch_task(
    cfg: &Config,
    pool: &SqlitePool,
    run_id: Uuid,
    watch: &WatchDef,
) -> Result<serde_json::Value, String> {
    match watch.task_type.as_str() {
        // "refresh" is the prior release's task_type for this same handler.
        // Accepted here (EXECUTION only) for back-compat so persisted rows keep
        // running; new creates still require "watch" (see SUPPORTED_TASK_TYPES).
        "watch" | "refresh" => orchestrate::run_url_watch(cfg, pool, run_id, watch).await,
        other => Err(format!("unsupported watch task_type: {other}")),
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;

//! Stale job watchdog: two-pass confirmation model for reclaiming stuck jobs.
//!
//! # State Machine
//!
//! ```text
//! running + updated_at old → [Pass 1] → running + result_json._watchdog (stale candidate)
//!                                                         ↓ (after confirm_secs)
//!                                                  [Pass 2] → failed + error_text='watchdog: reclaimed'
//! ```
//!
//! # Two-Pass Stale Detection
//!
//! The watchdog uses two sweeps to avoid false-positive reclaims:
//!
//! **Pass 1** (mark phase): Finds jobs with `updated_at` older than `idle_timeout_secs`
//! and writes `result_json._watchdog = { first_seen_stale_at, observed_updated_at }`.
//! Does NOT change status yet.
//!
//! **Pass 2** (confirm phase): On the *next* sweep, finds jobs still marked as stale
//! candidates. If `first_seen_stale_at` is older than `confirm_secs` AND `observed_updated_at`
//! still matches (no heartbeat arrived), promotes them to `status='failed'`.
//!
//! This prevents false positives when a heartbeat arrives between the two sweeps:
//! if the job's `updated_at` is refreshed between Pass 1 and Pass 2, the observed
//! timestamp won't match and Pass 2 skips it.
//!
//! **Gap analysis**: A job that fails to heartbeat for `idle_timeout_secs + confirm_secs`
//! will be reclaimed. With defaults (300s + 60s = 360s), this is 6 minutes.
//!
//! # RFC3339 Timestamp Format
//!
//! All timestamps use `Utc::now().to_rfc3339()` and are compared via
//! `DateTime::parse_from_rfc3339()` to handle format variations (Z vs +00:00,
//! variable microsecond precision) across Postgres TIMESTAMPTZ roundtrips.

use crate::crates::jobs::status::JobStatus;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::JobTable;

/// Maximum number of times a stale job is reset to `pending` before it is
/// permanently marked `failed`. Prevents a job that always crashes at the
/// same point from looping indefinitely.
pub const MAX_WATCHDOG_RECLAIM_ATTEMPTS: u64 = 3;

/// Extract the current reclaim attempt count from `result_json._reclaim.count`.
fn watchdog_reclaim_count(result_json: &Value) -> u64 {
    result_json
        .get("_reclaim")
        .and_then(|r| r.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Build the `result_json` payload for a job being reset to `pending` after
/// watchdog reclaim. Removes the `_watchdog` marker and increments `_reclaim.count`.
fn watchdog_reclaim_payload(mut result_json: Value, current_count: u64) -> Value {
    if !result_json.is_object() {
        result_json = serde_json::json!({});
    }
    if let Some(obj) = result_json.as_object_mut() {
        obj.remove("_watchdog");
        obj.insert(
            "_reclaim".to_string(),
            serde_json::json!({
                "count": current_count + 1,
                "last_reclaimed_at": Utc::now().to_rfc3339(),
            }),
        );
    }
    result_json
}

#[derive(Debug, Clone, Default)]
pub struct WatchdogSweepStats {
    pub stale_candidates: u64,
    pub marked_candidates: u64,
    /// Jobs reset to `pending` and re-enqueued (caller must publish to AMQP).
    pub reclaimed_jobs: u64,
    pub reclaimed_ids: Vec<Uuid>,
    /// Jobs permanently marked `failed` after exhausting reclaim attempts.
    pub exhausted_jobs: u64,
    pub exhausted_ids: Vec<Uuid>,
}

pub(crate) fn stale_watchdog_payload(
    mut result_json: Value,
    observed_updated_at: DateTime<Utc>,
) -> Value {
    if !result_json.is_object() {
        result_json = serde_json::json!({});
    }
    if let Some(obj) = result_json.as_object_mut() {
        // Preserve first_seen_stale_at if already set for the same observed_updated_at,
        // so the confirmation timer isn't reset on every sweep.
        let existing_first_seen = obj
            .get("_watchdog")
            .and_then(|w| {
                // Parse stored timestamp and compare as DateTime<Utc> to avoid
                // RFC3339 format drift (Z vs +00:00, microsecond precision) across
                // Postgres TIMESTAMPTZ roundtrips silently breaking the timer.
                let stored_str = w.get("observed_updated_at").and_then(|v| v.as_str())?;
                let stored_dt = DateTime::parse_from_rfc3339(stored_str).ok()?;
                if stored_dt.with_timezone(&Utc) == observed_updated_at {
                    w.get("first_seen_stale_at")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        obj.insert(
            "_watchdog".to_string(),
            serde_json::json!({
                "first_seen_stale_at": existing_first_seen,
                "observed_updated_at": observed_updated_at.to_rfc3339(),
            }),
        );
    }
    result_json
}

pub(crate) fn stale_watchdog_confirmed(
    result_json: &Value,
    observed_updated_at: DateTime<Utc>,
    confirm_secs: i64,
) -> bool {
    let Some(watchdog) = result_json.get("_watchdog") else {
        return false;
    };
    let Some(observed) = watchdog
        .get("observed_updated_at")
        .and_then(|value| value.as_str())
    else {
        return false;
    };
    // Parse stored timestamp and compare as DateTime<Utc> — avoids RFC3339 format
    // drift (Z vs +00:00, variable microsecond precision) across Postgres roundtrips.
    let Ok(stored_dt) = DateTime::parse_from_rfc3339(observed) else {
        return false;
    };
    if stored_dt.with_timezone(&Utc) != observed_updated_at {
        return false;
    }
    let Some(first_seen) = watchdog
        .get("first_seen_stale_at")
        .and_then(|value| value.as_str())
    else {
        return false;
    };
    let Ok(first_seen_at) = DateTime::parse_from_rfc3339(first_seen) else {
        return false;
    };
    let elapsed = Utc::now()
        .signed_duration_since(first_seen_at.with_timezone(&Utc))
        .num_seconds();
    elapsed >= confirm_secs
}

/// Reclaim stale running jobs with a two-pass confirmation model.
///
/// Pass 1: mark stale candidate in `result_json._watchdog` with observed `updated_at`.
/// Pass 2: if still stale and unchanged after `confirm_secs`, mark as failed.
pub async fn reclaim_stale_running_jobs(
    pool: &PgPool,
    table: JobTable,
    job_kind: &str,
    idle_timeout_secs: i64,
    confirm_secs: i64,
    marker: &str,
) -> Result<WatchdogSweepStats> {
    let select_query = format!(
        r#"
        SELECT id, updated_at, result_json
        FROM {}
        WHERE status = '{running}'
          AND updated_at < NOW() - make_interval(secs => $1::int)
        ORDER BY updated_at ASC
        LIMIT 50
        "#,
        table.as_str(),
        running = JobStatus::Running.as_str(),
    );
    let rows = sqlx::query(&select_query)
        .bind(idle_timeout_secs.min(i32::MAX as i64) as i32)
        .fetch_all(pool)
        .await?;

    let mut stats = WatchdogSweepStats {
        stale_candidates: rows.len() as u64,
        ..Default::default()
    };

    // Partition into confirmed (ready to reclaim) vs candidates (need marking).
    // Confirmed jobs are further split: retry (reset to pending) vs exhausted (mark failed).
    let mut retry_batch: Vec<(Uuid, Value)> = Vec::new();
    let mut exhausted_ids: Vec<Uuid> = Vec::new();
    let mut mark_batch: Vec<(Uuid, Value)> = Vec::new();

    for row in &rows {
        let id: Uuid = row.try_get("id")?;
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        let result_json: Option<Value> = row.try_get("result_json")?;
        let current_json = result_json.unwrap_or_else(|| serde_json::json!({}));

        if stale_watchdog_confirmed(&current_json, updated_at, confirm_secs) {
            let count = watchdog_reclaim_count(&current_json);
            if count >= MAX_WATCHDOG_RECLAIM_ATTEMPTS {
                exhausted_ids.push(id);
            } else {
                let new_json = watchdog_reclaim_payload(current_json, count);
                retry_batch.push((id, new_json));
            }
        } else {
            let marked = stale_watchdog_payload(current_json, updated_at);
            mark_batch.push((id, marked));
        }
    }

    // Reset retryable jobs to `pending` so the worker lane re-picks them up.
    // Caller is responsible for re-publishing IDs to AMQP via batch_enqueue_jobs.
    for (id, new_json) in retry_batch {
        let retry_query = format!(
            "UPDATE {} SET status='{pending}', updated_at=NOW(), \
             started_at=NULL, finished_at=NULL, error_text=NULL, result_json=$2 \
             WHERE id=$1 AND status='{running}' \
             RETURNING id",
            table.as_str(),
            pending = JobStatus::Pending.as_str(),
            running = JobStatus::Running.as_str(),
        );
        let maybe_row = sqlx::query(&retry_query)
            .bind(id)
            .bind(new_json)
            .fetch_optional(pool)
            .await?;
        if maybe_row.is_some() {
            stats.reclaimed_jobs += 1;
            stats.reclaimed_ids.push(id);
        }
    }

    // Permanently fail jobs that have exhausted reclaim attempts.
    if !exhausted_ids.is_empty() {
        let msg = format!(
            "watchdog: max reclaim attempts ({MAX_WATCHDOG_RECLAIM_ATTEMPTS}) exceeded \
             for {} job (marker={})",
            job_kind, marker
        );
        let fail_query = format!(
            "UPDATE {} SET status='{failed}', updated_at=NOW(), finished_at=NOW(), error_text=$2 \
             WHERE id = ANY($1) AND status='{running}' \
             RETURNING id",
            table.as_str(),
            failed = JobStatus::Failed.as_str(),
            running = JobStatus::Running.as_str(),
        );
        let exhausted: Vec<(Uuid,)> = sqlx::query_as(&fail_query)
            .bind(&exhausted_ids)
            .bind(&msg)
            .fetch_all(pool)
            .await?;
        stats.exhausted_jobs = exhausted.len() as u64;
        stats.exhausted_ids = exhausted.into_iter().map(|(id,)| id).collect();
    }

    // Mark candidates individually (each gets a unique payload with timestamps).
    for (id, marked) in mark_batch {
        let mark_query = format!(
            "UPDATE {} SET result_json=$2 WHERE id=$1 AND status='{running}'",
            table.as_str(),
            running = JobStatus::Running.as_str()
        );
        let _ = sqlx::query(&mark_query)
            .bind(id)
            .bind(marked)
            .execute(pool)
            .await?;
        stats.marked_candidates += 1;
    }

    Ok(stats)
}

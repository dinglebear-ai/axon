//! Job-claiming for the unified worker: the capacity-gated SELECT+UPDATE
//! that flips a queued job to `running`. Split out of `unified.rs` to keep
//! it under the monolith line cap.

use axon_api::source::{ApiError, AuthSnapshot, JobId, Timestamp};
use sqlx::{Row, SqlitePool};

use super::UnifiedClaimedJob;
use super::helpers::{json_error, parse_enum, parse_uuid, sql_error};
use crate::unified::retry_job_write;

/// Test-only entry point: production code claims via the poll loop in
/// [`super::unified_worker_loop`]; tests use this to claim+run one job
/// deterministically. Always allows claiming a `Source` job — callers that
/// need to exercise the source-lane gate use
/// [`claim_next_unified_job_unchecked`] directly.
#[allow(dead_code)]
pub(crate) async fn claim_next_unified_job(
    pool: &SqlitePool,
) -> Result<Option<UnifiedClaimedJob>, ApiError> {
    retry_job_write("unified worker claim", || {
        claim_next_unified_job_unchecked(pool, true)
    })
    .await
}

/// Claim the next eligible job. When `allow_source` is `false`, `Source`-kind
/// rows are excluded from selection entirely — the caller has already
/// established (via a non-blocking source-semaphore attempt) that it cannot
/// currently run one, so a `Source` row is left `queued` for a later pass
/// rather than being flipped to `running` with nowhere to run it. This lets
/// a full source lane be skipped over in favor of any other eligible kind,
/// instead of blocking the claim loop or claiming work that can't start.
pub(super) async fn claim_next_unified_job_unchecked(
    pool: &SqlitePool,
    allow_source: bool,
) -> Result<Option<UnifiedClaimedJob>, ApiError> {
    let mut tx = pool.begin().await.map_err(sql_error)?;
    let now = chrono::Utc::now().to_rfc3339();
    let row = sqlx::query(
        "SELECT job_id, kind, attempt, request_json, auth_snapshot_json
         FROM jobs
         WHERE status IN ('queued', 'waiting', 'blocked')
           AND (cooldown_until IS NULL OR cooldown_until <= ?)
           AND (kind <> 'source' OR ? = 1)
         ORDER BY
           CASE priority
             WHEN 'interactive' THEN 0
             WHEN 'high' THEN 1
             WHEN 'normal' THEN 2
             WHEN 'background' THEN 3
             WHEN 'maintenance' THEN 4
             ELSE 5
           END,
           updated_at ASC,
           job_id ASC
         LIMIT 1",
    )
    .bind(now.as_str())
    .bind(allow_source as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sql_error)?;

    let Some(row) = row else {
        tx.commit().await.map_err(sql_error)?;
        return Ok(None);
    };

    let job_id = JobId::new(parse_uuid(row.get::<String, _>("job_id"))?);
    let kind = parse_enum(row.get::<String, _>("kind"))?;
    let attempt = (row.get::<i64, _>("attempt") as u32).max(1);
    let request_json = row
        .get::<Option<String>, _>("request_json")
        .map(|value| serde_json::from_str(&value).map_err(json_error))
        .transpose()?;
    let auth_snapshot: AuthSnapshot =
        serde_json::from_str(&row.get::<String, _>("auth_snapshot_json")).map_err(json_error)?;
    let now = Timestamp::from(chrono::Utc::now());

    // Claiming a job always moves it to Running, so cooldown_until (only ever
    // meaningful while a job sits in Waiting) is cleared here too — this is a
    // direct SQL write, not routed through update_job_status's CASE-based
    // clear, so it needs its own.
    let result = sqlx::query(
        "UPDATE jobs SET
            status = 'running',
            phase = 'planning',
            attempt = ?,
            started_at = COALESCE(started_at, ?),
            updated_at = ?,
            cooldown_until = NULL
         WHERE job_id = ? AND status IN ('queued', 'waiting', 'blocked')",
    )
    .bind(attempt as i64)
    .bind(now.0.as_str())
    .bind(now.0.as_str())
    .bind(job_id.0.to_string())
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;

    if result.rows_affected() == 0 {
        tx.commit().await.map_err(sql_error)?;
        return Ok(None);
    }

    sqlx::query(
        "INSERT INTO job_attempts (
            attempt_id, job_id, attempt, status, worker_id, started_at, heartbeat_at
         ) VALUES (?, ?, ?, 'running', NULL, ?, ?)
         ON CONFLICT(job_id, attempt) DO UPDATE SET
            status = 'running',
            started_at = COALESCE(job_attempts.started_at, excluded.started_at),
            heartbeat_at = excluded.heartbeat_at",
    )
    .bind(format!("{}:{}", job_id.0, attempt))
    .bind(job_id.0.to_string())
    .bind(attempt as i64)
    .bind(now.0.as_str())
    .bind(now.0.as_str())
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;

    tx.commit().await.map_err(sql_error)?;
    Ok(Some(UnifiedClaimedJob {
        job_id,
        kind,
        attempt,
        request_json,
        auth_snapshot,
    }))
}

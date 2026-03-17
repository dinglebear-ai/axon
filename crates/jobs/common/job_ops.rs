//! Job lifecycle operations: claim, complete, fail, cancel, heartbeat.

use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::jobs::status::JobStatus;
use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use uuid::Uuid;

use super::JobTable;

/// Atomically claim the next pending job from the given table.
///
/// # Concurrency Safety
///
/// Uses `FOR UPDATE SKIP LOCKED`: multiple workers can call this concurrently
/// without blocking each other. Workers that encounter a locked row skip it
/// and move to the next available pending job. This guarantees:
/// - No two workers process the same job
/// - No worker is blocked waiting for another worker's lock
/// - The `UPDATE` (status → 'running') is atomic with the claim
pub async fn claim_next_pending(pool: &PgPool, table: JobTable) -> Result<Option<Uuid>> {
    let table = table.as_str();
    let query = format!(
        r#"WITH n AS (
            SELECT id FROM {table} WHERE status='{pending}' ORDER BY created_at ASC FOR UPDATE SKIP LOCKED LIMIT 1
        )
        UPDATE {table} j SET status='{running}', updated_at=NOW(), started_at=COALESCE(started_at, NOW())
        FROM n WHERE j.id=n.id RETURNING j.id"#,
        pending = JobStatus::Pending.as_str(),
        running = JobStatus::Running.as_str(),
    );
    let row = sqlx::query_as::<_, (Uuid,)>(&query)
        .fetch_optional(pool)
        .await
        .with_context(|| format!("claim_next_pending in {table}"))?;
    if let Some((id,)) = row {
        log_info(&format!("job_started job_id={id} table={table}"));
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

/// Claim a specific pending job by ID.
pub async fn claim_pending_by_id(pool: &PgPool, table: JobTable, id: Uuid) -> Result<bool> {
    let table = table.as_str();
    let query = format!(
        "UPDATE {table} SET status='{running}', updated_at=NOW(), started_at=COALESCE(started_at, NOW()), error_text=NULL WHERE id=$1 AND status='{pending}'",
        running = JobStatus::Running.as_str(),
        pending = JobStatus::Pending.as_str(),
    );
    let updated = sqlx::query(&query)
        .bind(id)
        .execute(pool)
        .await
        .with_context(|| format!("claim_pending_by_id for job {id} in {table}"))?
        .rows_affected();
    if updated > 0 {
        log_info(&format!("job_started job_id={id} table={table}"));
    }
    Ok(updated > 0)
}

/// Mark a running job as failed with an error message.
///
/// # Errors
///
/// Returns `Err` if the database query fails (e.g. pool exhausted, connection dropped).
/// Callers that want fire-and-forget behavior can use
/// `.unwrap_or_else(|e| log_warn(&format!("mark_job_failed: {e}")))`.
pub async fn mark_job_failed(
    pool: &PgPool,
    table: JobTable,
    id: Uuid,
    error_text: &str,
) -> Result<()> {
    let table_name = table.as_str();
    let query = format!(
        "UPDATE {table_name} SET status='{failed}', updated_at=NOW(), finished_at=NOW(), error_text=$2 WHERE id=$1 AND status='{running}'",
        failed = JobStatus::Failed.as_str(),
        running = JobStatus::Running.as_str(),
    );
    let result = sqlx::query(&query)
        .bind(id)
        .bind(error_text)
        .execute(pool)
        .await
        .with_context(|| format!("mark_job_failed for job {id} in {table_name}"))?;
    if result.rows_affected() > 0 {
        log_warn(&format!(
            "job_failed job_id={id} table={table_name} reason={error_text}"
        ));
    }
    Ok(())
}

/// Mark a running job as completed.
///
/// If `result_json` is provided, it is written to the job row; otherwise the
/// existing result payload is preserved.
///
/// # Idempotency Contract
///
/// If called twice for the same job, the second call returns `Ok(false)` — the
/// `WHERE status='running'` guard prevents double-completion. This is safe to call
/// from both the job handler and the watchdog.
pub async fn mark_job_completed(
    pool: &PgPool,
    table: JobTable,
    id: Uuid,
    result_json: Option<&Value>,
) -> Result<bool> {
    let table_name = table.as_str();
    let query = format!(
        "UPDATE {table_name} \
         SET status='{completed}', updated_at=NOW(), finished_at=NOW(), error_text=NULL, result_json=COALESCE($2, result_json) \
         WHERE id=$1 AND status='{running}'",
        completed = JobStatus::Completed.as_str(),
        running = JobStatus::Running.as_str(),
    );
    let rows = sqlx::query(&query)
        .bind(id)
        .bind(result_json)
        .execute(pool)
        .await
        .with_context(|| format!("mark_job_completed for job {id} in {table_name}"))?
        .rows_affected();
    if rows > 0 {
        log_info(&format!("job_completed job_id={id} table={table_name}"));
    }
    Ok(rows > 0)
}

/// Cancel a pending or running job.
///
/// # Behavior
///
/// Cancels a job if and only if it is in `pending` or `running` state.
/// Returns `Ok(true)` if the job was canceled, `Ok(false)` if the job was
/// already in a terminal state (`completed`, `failed`, `canceled`).
///
/// This is safe to call concurrently — only one caller will get `true`.
pub async fn cancel_pending_or_running_job(
    pool: &PgPool,
    table: JobTable,
    id: Uuid,
) -> Result<bool> {
    let table_name = table.as_str();
    let query = format!(
        "UPDATE {table_name} \
         SET status='{canceled}', updated_at=NOW(), finished_at=NOW() \
         WHERE id=$1 AND status IN ('{pending}','{running}')",
        canceled = JobStatus::Canceled.as_str(),
        pending = JobStatus::Pending.as_str(),
        running = JobStatus::Running.as_str(),
    );
    let rows = sqlx::query(&query)
        .bind(id)
        .execute(pool)
        .await
        .with_context(|| format!("cancel_pending_or_running_job for job {id} in {table_name}"))?
        .rows_affected();
    if rows > 0 {
        log_info(&format!("job_canceled job_id={id} table={table_name}"));
    }
    Ok(rows > 0)
}

/// Touch a running job heartbeat by updating `updated_at`.
///
/// # Watchdog Relationship
///
/// `touch_running_job` updates `updated_at` to `NOW()`. The watchdog
/// (`reclaim_stale_running_jobs`) marks jobs as stale when their `updated_at`
/// is older than `stale_timeout_secs`. Heartbeat tasks must call this at
/// intervals shorter than `stale_timeout_secs` to keep long-running jobs alive.
///
/// # Idempotency
///
/// Only updates rows WHERE `status='running'`. Calling this on a completed or
/// failed job is a no-op (no error, no rows affected).
pub async fn touch_running_job(pool: &PgPool, table: JobTable, id: Uuid) -> Result<()> {
    let table_name = table.as_str();
    let query = format!(
        "UPDATE {table_name} SET updated_at=NOW() WHERE id=$1 AND status='{running}'",
        running = JobStatus::Running.as_str(),
    );
    sqlx::query(&query)
        .bind(id)
        .execute(pool)
        .await
        .with_context(|| format!("touch_running_job for job {id} in {table_name}"))?;
    Ok(())
}

/// Spawn a background heartbeat task that calls [`touch_running_job`] on `interval_secs`
/// cadence until the returned sender signals stop.
///
/// # Usage
///
/// ```ignore
/// let (stop_tx, heartbeat) = spawn_heartbeat_task(pool.clone(), TABLE, id, 15);
/// // ... do work ...
/// let _ = stop_tx.send(true);
/// let _ = heartbeat.await;
/// ```
///
/// Each worker defines its own interval constant (e.g. 15s for embed, 30s for extract).
pub fn spawn_heartbeat_task(
    pool: PgPool,
    table: JobTable,
    id: Uuid,
    interval_secs: u64,
) -> (watch::Sender<bool>, JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let _ = touch_running_job(&pool, table, id).await;
                }
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
    (stop_tx, handle)
}

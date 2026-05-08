use sqlx::SqlitePool;
use uuid::Uuid;

use crate::jobs::backend::JobKind;
use crate::jobs::lite::store::now_ms;

use super::retry::retry_busy;

/// Atomically claim the oldest pending job in `table`.
/// Returns None if no pending jobs exist.
///
/// Wrapped in `retry_busy` so transient SQLite lock contention between
/// concurrent workers (BEGIN IMMEDIATE collisions) backs off and retries
/// before surfacing the error.
pub async fn claim_next_pending(
    pool: &SqlitePool,
    kind: JobKind,
) -> Result<Option<Uuid>, sqlx::Error> {
    retry_busy("claim_next_pending", || {
        claim_next_pending_inner(pool, kind)
    })
    .await
}

async fn claim_next_pending_inner(
    pool: &SqlitePool,
    kind: JobKind,
) -> Result<Option<Uuid>, sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let mut conn = pool.acquire().await?;

    // BEGIN IMMEDIATE acquires a write lock upfront under WAL mode, serializing
    // the SELECT+UPDATE atomically and eliminating TOCTOU contention between
    // concurrent workers.
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let row: Option<(String,)> = match sqlx::query_as(&format!(
        "SELECT id FROM {} WHERE status='pending' ORDER BY created_at LIMIT 1",
        table
    ))
    .fetch_optional(&mut *conn)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e);
        }
    };

    match row {
        None => {
            sqlx::query("ROLLBACK").execute(&mut *conn).await?;
            Ok(None)
        }
        Some((id_str,)) => {
            let update_result = match sqlx::query(&format!(
                "UPDATE {} SET status='running', started_at=?, updated_at=?, error_text=NULL \
                 WHERE id=? AND status='pending'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(&id_str)
            .execute(&mut *conn)
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    return Err(e);
                }
            };

            if update_result.rows_affected() == 0 {
                tracing::trace!(table, job_id = id_str, "claim lost to concurrent worker");
                sqlx::query("ROLLBACK").execute(&mut *conn).await?;
                return Ok(None);
            }

            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(Some(
                Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            ))
        }
    }
}

/// Mark a running job as completed. No-op if job is not in 'running' state.
pub async fn mark_completed(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    result_json: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    retry_busy("mark_completed", || {
        mark_completed_inner(pool, kind, id, result_json)
    })
    .await
}

async fn mark_completed_inner(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    result_json: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let result = match result_json {
        Some(result) => {
            sqlx::query(&format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=?, result_json=?, error_text=NULL \
                 WHERE id=? AND status='running'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(result.to_string())
            .bind(id.to_string())
            .execute(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=?, error_text=NULL \
                 WHERE id=? AND status='running'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(id.to_string())
            .execute(pool)
            .await?
        }
    };
    if result.rows_affected() == 0 {
        tracing::warn!(
            id = %id,
            table = table,
            "mark_completed: job row not found or not in running state (may have been canceled)"
        );
    }
    Ok(())
}

/// Bump only `updated_at` for a running job. Used by the periodic heartbeat
/// task so the watchdog's stale detection (driven by `updated_at`) does not
/// reclaim long-running jobs that haven't emitted a progress update recently.
///
/// Unlike [`update_result_json`], this does NOT touch `result_json` — that
/// avoids racing with progress persisters that own that column.
///
/// No-op (rows_affected=0) for jobs not in `running` state.
pub async fn touch_heartbeat(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    retry_busy("touch_heartbeat", || async {
        let now = now_ms();
        let table = kind.table_name();
        sqlx::query(&format!(
            "UPDATE {} SET updated_at=? WHERE id=? AND status='running'",
            table
        ))
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    })
    .await
}

/// Persist live job progress/result JSON without changing job status.
pub async fn update_result_json(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    result_json: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    retry_busy("update_result_json", || async {
        let now = now_ms();
        let table = kind.table_name();
        sqlx::query(&format!(
            "UPDATE {} SET result_json=?, updated_at=? WHERE id=?",
            table
        ))
        .bind(result_json.to_string())
        .bind(now)
        .bind(id.to_string())
        .execute(pool)
        .await?;
        Ok(())
    })
    .await
}

/// Mark a running job as failed with an error message.
pub async fn mark_failed(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    retry_busy("mark_failed", || mark_failed_inner(pool, kind, id, error)).await
}

async fn mark_failed_inner(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='failed', finished_at=?, updated_at=?, error_text=? \
         WHERE id=? AND status='running'",
        table
    ))
    .bind(now)
    .bind(now)
    .bind(error)
    .bind(id.to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        tracing::warn!(
            id = %id,
            table = table,
            "mark_failed: job row not found or not in running state (may have been canceled)"
        );
    }
    Ok(())
}

/// Set a job's status to 'canceled'. Works on pending or running jobs.
/// Returns true if a row was updated, false otherwise.
pub async fn cancel_row(pool: &SqlitePool, kind: JobKind, id: Uuid) -> Result<bool, sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='canceled', updated_at=?, finished_at=? \
         WHERE id=? AND status IN ('pending','running')",
        table
    ))
    .bind(now)
    .bind(now)
    .bind(id.to_string())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

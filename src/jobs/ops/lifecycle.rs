use sqlx::SqlitePool;
use uuid::Uuid;

use crate::jobs::backend::JobKind;
use crate::jobs::store::now_ms;

use super::retry::retry_busy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub attempt_id: String,
    pub attempt_count: i64,
}

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
    Ok(claim_next_pending_for_attempt(pool, kind)
        .await?
        .map(|claimed| claimed.id))
}

pub async fn claim_next_pending_for_attempt(
    pool: &SqlitePool,
    kind: JobKind,
) -> Result<Option<ClaimedJob>, sqlx::Error> {
    retry_busy("claim_next_pending_for_attempt", || {
        claim_next_pending_for_attempt_inner(pool, kind)
    })
    .await
}

async fn claim_next_pending_for_attempt_inner(
    pool: &SqlitePool,
    kind: JobKind,
) -> Result<Option<ClaimedJob>, sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let mut conn = pool.acquire().await?;

    // BEGIN IMMEDIATE acquires a write lock upfront under WAL mode, serializing
    // the SELECT+UPDATE atomically and eliminating TOCTOU contention between
    // concurrent workers.
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let row: Option<(String, Option<String>, i64)> = match sqlx::query_as(&format!(
        "SELECT id, error_text, attempt_count FROM {} WHERE status='pending' ORDER BY created_at LIMIT 1",
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
        Some((id_str, error_text, previous_attempt_count)) => {
            let attempt_id = Uuid::new_v4().to_string();
            let attempt_count = previous_attempt_count + 1;
            let update_result = match sqlx::query(&format!(
                "UPDATE {} SET status='running', started_at=?, updated_at=?, finished_at=NULL, \
                 attempt_count=?, active_attempt_id=? \
                 WHERE id=? AND status='pending'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(attempt_count)
            .bind(&attempt_id)
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
            if error_text.is_some() {
                tracing::info!(
                    table,
                    job_id = %id_str,
                    previous_error = error_text.as_deref().unwrap_or_default(),
                    "claiming pending job with existing recovery marker"
                );
            }
            Ok(Some(ClaimedJob {
                id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                attempt_id,
                attempt_count,
            }))
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
    mark_completed_for_attempt(pool, kind, id, None, result_json).await
}

pub async fn mark_completed_for_attempt(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
    result_json: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    retry_busy("mark_completed_for_attempt", || {
        mark_completed_inner(pool, kind, id, attempt_id, result_json)
    })
    .await
}

async fn mark_completed_inner(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
    result_json: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let result = match result_json {
        Some(result) => {
            let sql = format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=?, result_json=?, error_text=NULL, active_attempt_id=NULL \
                 WHERE id=? AND status='running'{}",
                table,
                attempt_clause(attempt_id)
            );
            let mut query = sqlx::query(&sql);
            query = query
                .bind(now)
                .bind(now)
                .bind(result.to_string())
                .bind(id.to_string());
            if let Some(attempt_id) = attempt_id {
                query = query.bind(attempt_id);
            }
            query.execute(pool).await?
        }
        None => {
            let sql = format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=?, error_text=NULL, active_attempt_id=NULL \
                 WHERE id=? AND status='running'{}",
                table,
                attempt_clause(attempt_id)
            );
            let mut query = sqlx::query(&sql);
            query = query.bind(now).bind(now).bind(id.to_string());
            if let Some(attempt_id) = attempt_id {
                query = query.bind(attempt_id);
            }
            query.execute(pool).await?
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
    touch_heartbeat_for_attempt(pool, kind, id, None).await
}

pub async fn touch_heartbeat_for_attempt(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    retry_busy("touch_heartbeat_for_attempt", || async {
        let now = now_ms();
        let table = kind.table_name();
        let sql = format!(
            "UPDATE {} SET updated_at=? WHERE id=? AND status='running'{}",
            table,
            attempt_clause(attempt_id)
        );
        let mut query = sqlx::query(&sql);
        query = query.bind(now).bind(id.to_string());
        if let Some(attempt_id) = attempt_id {
            query = query.bind(attempt_id);
        }
        query.execute(pool).await?;
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
    update_result_json_for_attempt(pool, kind, id, None, result_json).await
}

pub async fn update_result_json_for_attempt(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
    result_json: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    retry_busy("update_result_json_for_attempt", || async {
        let now = now_ms();
        let table = kind.table_name();
        let sql = format!(
            "UPDATE {} SET result_json=?, updated_at=? WHERE id=? AND status='running'{}",
            table,
            attempt_clause(attempt_id)
        );
        let mut query = sqlx::query(&sql);
        query = query
            .bind(result_json.to_string())
            .bind(now)
            .bind(id.to_string());
        if let Some(attempt_id) = attempt_id {
            query = query.bind(attempt_id);
        }
        query.execute(pool).await.map(|result| {
            if result.rows_affected() == 0 {
                tracing::debug!(
                    table,
                    job_id = %id,
                    "progress update skipped because job is no longer running"
                );
            }
        })?;
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
    mark_failed_for_attempt(pool, kind, id, None, error).await
}

pub async fn mark_failed_for_attempt(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
    error: &str,
) -> Result<(), sqlx::Error> {
    retry_busy("mark_failed_for_attempt", || {
        mark_failed_inner(pool, kind, id, attempt_id, error)
    })
    .await
}

async fn mark_failed_inner(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
    attempt_id: Option<&str>,
    error: &str,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let sql = format!(
        "UPDATE {} SET status='failed', finished_at=?, updated_at=?, error_text=?, active_attempt_id=NULL \
         WHERE id=? AND status='running'{}",
        table,
        attempt_clause(attempt_id)
    );
    let mut query = sqlx::query(&sql);
    query = query.bind(now).bind(now).bind(error).bind(id.to_string());
    if let Some(attempt_id) = attempt_id {
        query = query.bind(attempt_id);
    }
    let result = query.execute(pool).await?;
    if result.rows_affected() == 0 {
        tracing::warn!(
            id = %id,
            table = table,
            "mark_failed: job row not found or not in running state (may have been canceled)"
        );
    }
    Ok(())
}

fn attempt_clause(attempt_id: Option<&str>) -> &'static str {
    if attempt_id.is_some() {
        " AND active_attempt_id=?"
    } else {
        ""
    }
}

/// Set a job's status to 'canceled'. Works on pending or running jobs.
/// Returns true if a row was updated, false otherwise.
pub async fn cancel_row(pool: &SqlitePool, kind: JobKind, id: Uuid) -> Result<bool, sqlx::Error> {
    let now = now_ms();
    let table = kind.table_name();
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='canceled', updated_at=?, finished_at=?, active_attempt_id=NULL \
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

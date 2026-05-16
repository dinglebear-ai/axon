use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::jobs::backend::{JobKind, JobStatusRow, JobSummary};
use crate::jobs::status::JobStatus;
use crate::services::types::ServiceJob;

use super::store::now_ms;

type JobStatusRowTuple = (
    String,
    String,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
    Option<i64>,
    Option<String>,
);

pub(crate) fn ms_to_dt(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_else(|| {
        tracing::warn!(
            raw_ms = ms,
            "invalid timestamp millis in job row, using epoch"
        );
        DateTime::default()
    })
}

/// Count all jobs in a table.
pub async fn count_jobs(pool: &SqlitePool, kind: JobKind) -> Result<i64, sqlx::Error> {
    let table = kind.table_name();
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// List all jobs in a table as summary rows (most recent first).
/// Returns at most 500 rows.
pub async fn list_jobs(pool: &SqlitePool, kind: JobKind) -> Result<Vec<JobSummary>, sqlx::Error> {
    // Different tables have different target columns.
    // This query checks which columns exist in the target table and uses the appropriate one.
    let query_str = match kind {
        JobKind::Embed => {
            "SELECT id, status, created_at, COALESCE(input_text, '') as target FROM axon_embed_jobs ORDER BY created_at DESC LIMIT 500"
        }
        JobKind::Extract => {
            "SELECT id, status, created_at, COALESCE(urls_json, '') as target FROM axon_extract_jobs ORDER BY created_at DESC LIMIT 500"
        }
        JobKind::Ingest => {
            "SELECT id, status, created_at, COALESCE(target, '') as target FROM axon_ingest_jobs ORDER BY created_at DESC LIMIT 500"
        }
        JobKind::Crawl => {
            "SELECT id, status, created_at, COALESCE(url, '') as target FROM axon_crawl_jobs ORDER BY created_at DESC LIMIT 500"
        }
    };

    let rows: Vec<(String, String, i64, String)> =
        sqlx::query_as(query_str).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(id, status, created_at, target)| JobSummary {
            id: Uuid::parse_str(&id).unwrap_or_else(|e| {
                tracing::warn!(raw = %id, error = %e, "corrupt UUID in job row, using nil");
                Uuid::nil()
            }),
            status: JobStatus::from_str(&status),
            created_at: ms_to_dt(created_at),
            target,
        })
        .collect())
}

/// Delete completed and failed jobs older than 24 hours.
/// Returns count of rows deleted.
pub async fn cleanup_jobs(pool: &SqlitePool, kind: JobKind) -> Result<u64, sqlx::Error> {
    let table = kind.table_name();
    let cutoff = now_ms() - 86_400_000;
    let result = sqlx::query(&format!(
        "DELETE FROM {} WHERE status IN ('completed','failed') AND finished_at < ?",
        table
    ))
    .bind(cutoff)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Delete ALL jobs in a table.
/// Returns count of rows deleted.
pub async fn clear_jobs(pool: &SqlitePool, kind: JobKind) -> Result<u64, sqlx::Error> {
    let table = kind.table_name();
    let result = sqlx::query(&format!("DELETE FROM {}", table))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Fetch a full status row for a job by ID.
/// Returns None if not found.
pub async fn job_status_row(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<JobStatusRow>, sqlx::Error> {
    let table = kind.table_name();
    let row: Option<JobStatusRowTuple> = sqlx::query_as(&format!(
        "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, result_json, \
         attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
         FROM {} WHERE id = ?",
        table
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(
            id,
            status,
            created_at,
            updated_at,
            started_at,
            finished_at,
            error_text,
            result_json,
            attempt_count,
            active_attempt_id,
            last_reclaimed_at,
            last_reclaimed_reason,
        )| {
            JobStatusRow {
                id: Uuid::parse_str(&id).unwrap_or_else(|e| {
                    tracing::warn!(raw = %id, error = %e, "corrupt UUID in job status row, using nil");
                    Uuid::nil()
                }),
                status: JobStatus::from_str(&status),
                created_at: ms_to_dt(created_at),
                updated_at: ms_to_dt(updated_at),
                started_at: started_at.map(ms_to_dt),
                finished_at: finished_at.map(ms_to_dt),
                error_text,
                result_json: result_json.and_then(|s| {
                    serde_json::from_str(&s).unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "corrupt result_json in job status row, using None");
                        None
                    })
                }),
                attempt_count,
                active_attempt_id,
                last_reclaimed_at: last_reclaimed_at.map(ms_to_dt),
                last_reclaimed_reason,
            }
        },
    ))
}

/// Get the error_text for a job.
/// Returns None if not found or no error.
pub async fn job_errors(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let table = kind.table_name();
    let row: Option<(Option<String>,)> =
        sqlx::query_as(&format!("SELECT error_text FROM {} WHERE id = ?", table))
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

    Ok(row.and_then(|(e,)| e))
}

#[derive(sqlx::FromRow)]
struct ServiceJobRow {
    id: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    error_text: Option<String>,
    url: Option<String>,
    source_type: Option<String>,
    target: Option<String>,
    urls_json: Option<String>,
    result_json: Option<String>,
    config_json: Option<String>,
    attempt_count: i64,
    active_attempt_id: Option<String>,
    last_reclaimed_at: Option<i64>,
    last_reclaimed_reason: Option<String>,
}

/// Returns the per-kind `SELECT … FROM <table>` fragment (no trailing clause).
/// The caller appends either a `WHERE id = ?` or `ORDER BY … LIMIT … OFFSET …`.
fn service_select_from(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Crawl => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             url, NULL as source_type, NULL as target, NULL as urls_json, result_json, config_json, \
             attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
             FROM axon_crawl_jobs"
        }
        JobKind::Embed => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, NULL as source_type, input_text as target, NULL as urls_json, result_json, config_json, \
             attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
             FROM axon_embed_jobs"
        }
        JobKind::Extract => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, NULL as source_type, NULL as target, urls_json, result_json, config_json, \
             attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
             FROM axon_extract_jobs"
        }
        JobKind::Ingest => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, source_type, target, NULL as urls_json, result_json, config_json, \
             attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
             FROM axon_ingest_jobs"
        }
    }
}

fn service_job_from_row(row: ServiceJobRow) -> ServiceJob {
    ServiceJob {
        id: Uuid::parse_str(&row.id).unwrap_or_else(|e| {
            tracing::warn!(raw = %row.id, error = %e, "corrupt UUID in service job row, using nil");
            Uuid::nil()
        }),
        status: row.status,
        created_at: ms_to_dt(row.created_at),
        updated_at: ms_to_dt(row.updated_at),
        started_at: row.started_at.map(ms_to_dt),
        finished_at: row.finished_at.map(ms_to_dt),
        error_text: row.error_text,
        url: row.url,
        source_type: row.source_type,
        target: row.target,
        urls_json: row.urls_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt urls_json in service job row, using None");
                None
            })
        }),
        result_json: row.result_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt result_json in service job row, using None");
                None
            })
        }),
        config_json: row.config_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt config_json in service job row, using None");
                None
            })
        }),
        attempt_count: row.attempt_count,
        active_attempt_id: row.active_attempt_id,
        last_reclaimed_at: row.last_reclaimed_at.map(ms_to_dt),
        last_reclaimed_reason: row.last_reclaimed_reason,
    }
}

pub async fn list_service_jobs(
    pool: &SqlitePool,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, sqlx::Error> {
    let order_by = "ORDER BY CASE status \
                WHEN 'running' THEN 0 \
                WHEN 'pending' THEN 1 \
                WHEN 'completed' THEN 2 \
                WHEN 'failed' THEN 3 \
                WHEN 'canceled' THEN 4 \
                ELSE 5 \
             END, \
             created_at DESC, \
             updated_at DESC, \
             id ASC";
    // SAFETY: service_select_from(kind) and order_by are compile-time `&'static
    // str` from a closed enum dispatch; no caller-controlled values reach this
    // format!(). Limit/offset are bound parameters.
    let query = format!(
        "{} {} LIMIT ?1 OFFSET ?2",
        service_select_from(kind),
        order_by,
    );
    let rows: Vec<ServiceJobRow> = sqlx::query_as(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(service_job_from_row).collect())
}

pub async fn list_ingest_service_jobs(
    pool: &SqlitePool,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, sqlx::Error> {
    let rows: Vec<ServiceJobRow> = sqlx::query_as(
        "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
         NULL as url, source_type, target, NULL as urls_json, result_json, config_json, \
         attempt_count, active_attempt_id, last_reclaimed_at, last_reclaimed_reason \
         FROM axon_ingest_jobs \
         WHERE (?1 IS NULL OR source_type = ?1) \
         ORDER BY CASE status \
           WHEN 'running' THEN 0 \
           WHEN 'pending' THEN 1 \
           WHEN 'completed' THEN 2 \
           WHEN 'failed' THEN 3 \
           WHEN 'canceled' THEN 4 \
           ELSE 5 \
         END, \
         created_at DESC, \
         updated_at DESC, \
         id \
         LIMIT ?2 OFFSET ?3",
    )
    .bind(source_filter)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(service_job_from_row).collect())
}

pub async fn service_job(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, sqlx::Error> {
    let query = format!("{} WHERE id = ?", service_select_from(kind));
    let row: Option<ServiceJobRow> = sqlx::query_as(&query)
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    Ok(row.map(service_job_from_row))
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;

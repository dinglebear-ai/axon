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
        "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, result_json \
         FROM {} WHERE id = ?",
        table
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(id, status, created_at, updated_at, started_at, finished_at, error_text, result_json)| {
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

type ServiceJobTuple = (
    String,
    String,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Returns the per-kind `SELECT … FROM <table>` fragment (no trailing clause).
/// The caller appends either a `WHERE id = ?` or `ORDER BY … LIMIT … OFFSET …`.
fn service_select_from(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Crawl => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             url, NULL as source_type, NULL as target, NULL as urls_json, result_json, config_json \
             FROM axon_crawl_jobs"
        }
        JobKind::Embed => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, NULL as source_type, input_text as target, NULL as urls_json, result_json, config_json \
             FROM axon_embed_jobs"
        }
        JobKind::Extract => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, NULL as source_type, NULL as target, urls_json, result_json, config_json \
             FROM axon_extract_jobs"
        }
        JobKind::Ingest => {
            "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
             NULL as url, source_type, target, NULL as urls_json, result_json, config_json \
             FROM axon_ingest_jobs"
        }
    }
}

fn service_job_from_tuple(row: ServiceJobTuple) -> ServiceJob {
    let (
        id,
        status,
        created_at,
        updated_at,
        started_at,
        finished_at,
        error_text,
        url,
        source_type,
        target,
        urls_json,
        result_json,
        config_json,
    ) = row;
    ServiceJob {
        id: Uuid::parse_str(&id).unwrap_or_else(|e| {
            tracing::warn!(raw = %id, error = %e, "corrupt UUID in service job row, using nil");
            Uuid::nil()
        }),
        status,
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        error_text,
        url,
        source_type,
        target,
        urls_json: urls_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt urls_json in service job row, using None");
                None
            })
        }),
        result_json: result_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt result_json in service job row, using None");
                None
            })
        }),
        config_json: config_json.and_then(|s| {
            serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "corrupt config_json in service job row, using None");
                None
            })
        }),
    }
}

pub async fn list_service_jobs(
    pool: &SqlitePool,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, sqlx::Error> {
    let query = format!(
        "{} ORDER BY created_at DESC, id LIMIT {} OFFSET {}",
        service_select_from(kind),
        limit,
        offset
    );
    let rows: Vec<ServiceJobTuple> = sqlx::query_as(&query).fetch_all(pool).await?;
    Ok(rows.into_iter().map(service_job_from_tuple).collect())
}

pub async fn list_ingest_service_jobs(
    pool: &SqlitePool,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, sqlx::Error> {
    let rows: Vec<ServiceJobTuple> = sqlx::query_as(
        "SELECT id, status, created_at, updated_at, started_at, finished_at, error_text, \
         NULL as url, source_type, target, NULL as urls_json, result_json, config_json \
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
    Ok(rows.into_iter().map(service_job_from_tuple).collect())
}

pub async fn service_job(
    pool: &SqlitePool,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, sqlx::Error> {
    let query = format!("{} WHERE id = ?", service_select_from(kind));
    let row: Option<ServiceJobTuple> = sqlx::query_as(&query)
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    Ok(row.map(service_job_from_tuple))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::backend::JobPayload;
    use crate::jobs::lite::ops::enqueue_job;
    use crate::jobs::lite::store::open_sqlite_pool;

    #[tokio::test]
    async fn list_jobs_returns_all_entries() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://a.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .unwrap();
        enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://b.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .unwrap();

        let jobs = list_jobs(&pool, JobKind::Crawl).await.unwrap();
        assert_eq!(jobs.len(), 2);
        // Both jobs have the same created_at (tight loop), so order is by insertion
        // Either order is acceptable; just check both exist
        let targets: std::collections::HashSet<_> =
            jobs.iter().map(|j| j.target.as_str()).collect();
        assert!(targets.contains("https://a.com"));
        assert!(targets.contains("https://b.com"));
    }

    #[tokio::test]
    async fn cleanup_removes_old_completed_jobs() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        let now = now_ms();
        let old_time = now - 100_000_000;
        sqlx::query(
            "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at, finished_at) \
             VALUES ('old-id', 'completed', 'https://old.com', '{}', ?, ?, ?)",
        )
        .bind(old_time)
        .bind(old_time)
        .bind(old_time)
        .execute(&pool)
        .await
        .unwrap();

        let deleted = cleanup_jobs(&pool, JobKind::Crawl).await.unwrap();
        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn job_status_row_returns_none_for_unknown_id() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        let id = Uuid::new_v4();
        let row = job_status_row(&pool, JobKind::Crawl, id).await.unwrap();
        assert!(row.is_none());
    }

    #[tokio::test]
    async fn list_ingest_service_jobs_applies_source_filter_before_limit() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        let now = now_ms();

        for i in 0..60 {
            sqlx::query(
                "INSERT INTO axon_ingest_jobs (id, status, source_type, target, config_json, created_at, updated_at) \
                 VALUES (?, 'completed', 'github', ?, '{}', ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(format!("owner/repo-{i}"))
            .bind(now + i)
            .bind(now + i)
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query(
            "INSERT INTO axon_ingest_jobs (id, status, source_type, target, config_json, created_at, updated_at) \
             VALUES (?, 'completed', 'sessions', 'sessions', '{}', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(now - 1)
        .bind(now - 1)
        .execute(&pool)
        .await
        .unwrap();

        let jobs = list_ingest_service_jobs(&pool, Some("sessions"), 50, 0)
            .await
            .unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source_type.as_deref(), Some("sessions"));
        assert_eq!(jobs[0].target.as_deref(), Some("sessions"));
    }
}

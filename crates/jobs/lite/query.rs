use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::crates::jobs::backend::{JobStatusRow, JobSummary};
use crate::crates::jobs::status::JobStatus;

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

fn ms_to_dt(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// List all jobs in a table as summary rows (most recent first).
/// Returns at most 500 rows.
pub async fn list_jobs(pool: &SqlitePool, table: &str) -> Result<Vec<JobSummary>, sqlx::Error> {
    // Different tables have different target columns.
    // This query checks which columns exist in the target table and uses the appropriate one.
    let query_str = match table {
        "axon_embed_jobs" => {
            "SELECT id, status, created_at, COALESCE(input_text, '') as target FROM axon_embed_jobs ORDER BY created_at DESC LIMIT 500"
        }
        "axon_extract_jobs" => {
            "SELECT id, status, created_at, COALESCE(urls_json, '') as target FROM axon_extract_jobs ORDER BY created_at DESC LIMIT 500"
        }
        "axon_ingest_jobs" => {
            "SELECT id, status, created_at, COALESCE(target, '') as target FROM axon_ingest_jobs ORDER BY created_at DESC LIMIT 500"
        }
        _ => {
            // crawl_jobs, refresh_jobs, graph_jobs use 'url' or have no target
            &format!(
                "SELECT id, status, created_at, COALESCE(url, '') as target FROM {} ORDER BY created_at DESC LIMIT 500",
                table
            )
        }
    };

    let rows: Vec<(String, String, i64, String)> =
        sqlx::query_as(query_str).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(id, status, created_at, target)| JobSummary {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            status: JobStatus::from_str(&status),
            created_at: ms_to_dt(created_at),
            target,
        })
        .collect())
}

/// Delete completed and failed jobs older than 24 hours.
/// Returns count of rows deleted.
pub async fn cleanup_jobs(pool: &SqlitePool, table: &str) -> Result<u64, sqlx::Error> {
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
pub async fn clear_jobs(pool: &SqlitePool, table: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(&format!("DELETE FROM {}", table))
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Fetch a full status row for a job by ID.
/// Returns None if not found.
pub async fn job_status_row(
    pool: &SqlitePool,
    table: &str,
    id: Uuid,
) -> Result<Option<JobStatusRow>, sqlx::Error> {
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
                id: Uuid::parse_str(&id).unwrap_or_default(),
                status: JobStatus::from_str(&status),
                created_at: ms_to_dt(created_at),
                updated_at: ms_to_dt(updated_at),
                started_at: started_at.map(ms_to_dt),
                finished_at: finished_at.map(ms_to_dt),
                error_text,
                result_json: result_json.and_then(|s| serde_json::from_str(&s).ok()),
            }
        },
    ))
}

/// Get the error_text for a job.
/// Returns None if not found or no error.
pub async fn job_errors(
    pool: &SqlitePool,
    table: &str,
    id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as(&format!("SELECT error_text FROM {} WHERE id = ?", table))
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;

    Ok(row.and_then(|(e,)| e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;

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

        let jobs = list_jobs(&pool, "axon_crawl_jobs").await.unwrap();
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

        let deleted = cleanup_jobs(&pool, "axon_crawl_jobs").await.unwrap();
        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn job_status_row_returns_none_for_unknown_id() {
        let pool = open_sqlite_pool(":memory:").await.unwrap();
        let id = Uuid::new_v4();
        let row = job_status_row(&pool, "axon_crawl_jobs", id).await.unwrap();
        assert!(row.is_none());
    }
}

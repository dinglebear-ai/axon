use sqlx::SqlitePool;
use uuid::Uuid;

use crate::crates::jobs::backend::JobPayload;
use crate::crates::jobs::lite::store::now_ms;

/// Check whether the pending job count for a given queue is at or above the configured cap.
///
/// `table` is the SQL table name (e.g. `axon_crawl_jobs`).
/// `queue_name` is the human-readable queue name used in error messages.
/// `limit` is the cap (0 = unlimited).
///
/// Returns `Err` with a human-readable message when the queue is full.
pub(super) async fn check_pending_cap_for(
    pool: &SqlitePool,
    table: &str,
    queue_name: &str,
    limit: u64,
) -> Result<(), sqlx::Error> {
    if limit == 0 {
        return Ok(());
    }
    let query = format!("SELECT COUNT(*) FROM {table} WHERE status = 'pending'");
    let count: i64 = sqlx::query_scalar(&query).fetch_one(pool).await?;
    if count as u64 >= limit {
        return Err(sqlx::Error::Configuration(
            format!(
                "{queue_name} queue is at capacity ({count} pending jobs, max {limit}); \
                 wait for workers to drain or raise the queue cap env var"
            )
            .into(),
        ));
    }
    Ok(())
}

/// Check whether the pending crawl job count is at or above the configured cap.
///
/// Reads `AXON_MAX_PENDING_CRAWL_JOBS` from the environment (default 100, 0 = unlimited).
async fn check_crawl_pending_cap(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let limit: u64 = std::env::var("AXON_MAX_PENDING_CRAWL_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    check_pending_cap_for(pool, "axon_crawl_jobs", "crawl", limit).await
}

/// Check whether the pending embed job count is at or above the configured cap.
///
/// Reads `AXON_MAX_PENDING_EMBED_JOBS` from the environment (default 50, 0 = unlimited).
async fn check_embed_pending_cap(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let limit: u64 = std::env::var("AXON_MAX_PENDING_EMBED_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    check_pending_cap_for(pool, "axon_embed_jobs", "embed", limit).await
}

/// Check whether the pending extract job count is at or above the configured cap.
///
/// Reads `AXON_MAX_PENDING_EXTRACT_JOBS` from the environment (default 50, 0 = unlimited).
async fn check_extract_pending_cap(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let limit: u64 = std::env::var("AXON_MAX_PENDING_EXTRACT_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    check_pending_cap_for(pool, "axon_extract_jobs", "extract", limit).await
}

/// Check whether the pending ingest job count is at or above the configured cap.
///
/// Reads `AXON_MAX_PENDING_INGEST_JOBS` from the environment (default 50, 0 = unlimited).
async fn check_ingest_pending_cap(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let limit: u64 = std::env::var("AXON_MAX_PENDING_INGEST_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    check_pending_cap_for(pool, "axon_ingest_jobs", "ingest", limit).await
}

/// Insert a new job row with status='pending'. Returns the new job's UUID.
pub async fn enqueue_job(pool: &SqlitePool, payload: &JobPayload) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let id_str = id.to_string();

    match payload {
        JobPayload::Crawl { url, config_json } => {
            check_crawl_pending_cap(pool).await?;
            sqlx::query(
                "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(url)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Embed { input, config_json } => {
            check_embed_pending_cap(pool).await?;
            sqlx::query(
                "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(input)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Extract { urls, config_json } => {
            check_extract_pending_cap(pool).await?;
            let urls_json =
                serde_json::to_string(urls).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO axon_extract_jobs (id, status, urls_json, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(&urls_json)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
        JobPayload::Ingest {
            target,
            source_type,
            config_json,
        } => {
            check_ingest_pending_cap(pool).await?;
            sqlx::query(
                "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(target)
            .bind(source_type)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(id)
}

use super::store::now_ms;
use crate::crates::jobs::backend::JobPayload;
use sqlx::SqlitePool;
use tracing;
use uuid::Uuid;

/// Insert a new job row with status='pending'. Returns the new job's UUID.
pub async fn enqueue_job(pool: &SqlitePool, payload: &JobPayload) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = now_ms();
    let id_str = id.to_string();

    match payload {
        JobPayload::Crawl { url, config_json } => {
            sqlx::query(
                "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)"
            )
            .bind(&id_str).bind(url).bind(config_json).bind(now).bind(now)
            .execute(pool).await?;
        }
        JobPayload::Embed { input, config_json } => {
            sqlx::query(
                "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)"
            )
            .bind(&id_str).bind(input).bind(config_json).bind(now).bind(now)
            .execute(pool).await?;
        }
        JobPayload::Extract { urls, config_json } => {
            let urls_json =
                serde_json::to_string(urls).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO axon_extract_jobs (id, status, urls_json, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)"
            )
            .bind(&id_str).bind(&urls_json).bind(config_json).bind(now).bind(now)
            .execute(pool).await?;
        }
        JobPayload::Ingest {
            target,
            source_type,
            config_json,
        } => {
            sqlx::query(
                "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?, ?)"
            )
            .bind(&id_str).bind(target).bind(source_type).bind(config_json).bind(now).bind(now)
            .execute(pool).await?;
        }
        JobPayload::Refresh { url, config_json } => {
            sqlx::query(
                "INSERT INTO axon_refresh_jobs (id, status, url, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?, ?)"
            )
            .bind(&id_str).bind(url).bind(config_json).bind(now).bind(now)
            .execute(pool).await?;
        }
        JobPayload::Graph { config_json } => {
            sqlx::query(
                "INSERT INTO axon_graph_jobs (id, status, config_json, created_at, updated_at) \
                 VALUES (?, 'pending', ?, ?, ?)",
            )
            .bind(&id_str)
            .bind(config_json)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }

    Ok(id)
}

/// Atomically claim the oldest pending job in `table`.
/// Returns None if no pending jobs exist.
pub async fn claim_next_pending(
    pool: &SqlitePool,
    table: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let now = now_ms();
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
                "UPDATE {} SET status='running', started_at=?, updated_at=? \
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
    table: &str,
    id: Uuid,
    result_json: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    match result_json {
        Some(result) => {
            sqlx::query(&format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=?, result_json=? \
                 WHERE id=? AND status='running'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(result.to_string())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(&format!(
                "UPDATE {} SET status='completed', finished_at=?, updated_at=? \
                 WHERE id=? AND status='running'",
                table
            ))
            .bind(now)
            .bind(now)
            .bind(id.to_string())
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// Mark a running job as failed with an error message.
pub async fn mark_failed(
    pool: &SqlitePool,
    table: &str,
    id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    sqlx::query(&format!(
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
    Ok(())
}

/// Bump updated_at for a running job (heartbeat keepalive).
pub async fn touch_running_job(
    pool: &SqlitePool,
    table: &str,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "UPDATE {} SET updated_at=? WHERE id=? AND status='running'",
        table
    ))
    .bind(now_ms())
    .bind(id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Set a job's status to 'canceled'. Works on pending or running jobs.
/// Returns true if a row was updated, false otherwise.
pub async fn cancel_row(pool: &SqlitePool, table: &str, id: Uuid) -> Result<bool, sqlx::Error> {
    let now = now_ms();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::store::open_sqlite_pool;
    use std::sync::Arc;

    async fn test_pool() -> SqlitePool {
        open_sqlite_pool(":memory:").await.expect("pool")
    }

    #[tokio::test]
    async fn enqueue_and_claim_crawl_job() {
        let pool = test_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");

        let claimed = claim_next_pending(&pool, "axon_crawl_jobs")
            .await
            .expect("claim");
        assert_eq!(claimed, Some(id));
    }

    #[tokio::test]
    async fn claim_returns_none_when_queue_empty() {
        let pool = test_pool().await;
        let claimed = claim_next_pending(&pool, "axon_crawl_jobs")
            .await
            .expect("claim");
        assert_eq!(claimed, None);
    }

    #[tokio::test]
    async fn mark_completed_updates_status() {
        let pool = test_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "test".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");
        claim_next_pending(&pool, "axon_embed_jobs")
            .await
            .expect("claim");
        mark_completed(&pool, "axon_embed_jobs", id, None)
            .await
            .expect("complete");

        let status: (String,) = sqlx::query_as("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
        assert_eq!(status.0, "completed");
    }

    #[tokio::test]
    async fn mark_failed_sets_error_text() {
        let pool = test_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://fail.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");
        claim_next_pending(&pool, "axon_crawl_jobs")
            .await
            .expect("claim");
        mark_failed(&pool, "axon_crawl_jobs", id, "connection timeout")
            .await
            .expect("fail");

        let row: (String, String) =
            sqlx::query_as("SELECT status, error_text FROM axon_crawl_jobs WHERE id = ?")
                .bind(id.to_string())
                .fetch_one(&pool)
                .await
                .expect("fetch");
        assert_eq!(row.0, "failed");
        assert_eq!(row.1, "connection timeout");
    }

    #[tokio::test]
    async fn mark_completed_preserves_existing_result_when_none_provided() {
        let pool = test_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "test".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");
        claim_next_pending(&pool, "axon_embed_jobs")
            .await
            .expect("claim");
        sqlx::query("UPDATE axon_embed_jobs SET result_json=? WHERE id=?")
            .bind(r#"{"phase":"running"}"#)
            .bind(id.to_string())
            .execute(&pool)
            .await
            .expect("seed result");

        mark_completed(&pool, "axon_embed_jobs", id, None)
            .await
            .expect("complete");

        let row: (String,) = sqlx::query_as("SELECT result_json FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
        assert_eq!(row.0, r#"{"phase":"running"}"#);
    }

    #[tokio::test]
    async fn cancel_row_sets_finished_at() {
        let pool = test_pool().await;
        let id = enqueue_job(
            &pool,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");

        let canceled = cancel_row(&pool, "axon_crawl_jobs", id)
            .await
            .expect("cancel");
        assert!(canceled);

        let row: (String, Option<i64>) =
            sqlx::query_as("SELECT status, finished_at FROM axon_crawl_jobs WHERE id = ?")
                .bind(id.to_string())
                .fetch_one(&pool)
                .await
                .expect("fetch");
        assert_eq!(row.0, "canceled");
        assert!(row.1.is_some());
    }

    #[tokio::test]
    async fn concurrent_claims_only_return_one_job() {
        let path = std::env::temp_dir()
            .join(format!("axon-lite-claim-{}.db", Uuid::new_v4()))
            .to_string_lossy()
            .into_owned();
        let pool_a = Arc::new(open_sqlite_pool(&path).await.expect("pool a"));
        let pool_b = Arc::new(open_sqlite_pool(&path).await.expect("pool b"));

        let id = enqueue_job(
            &pool_a,
            &JobPayload::Crawl {
                url: "https://example.com".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .expect("enqueue");

        async fn claim_with_lock_retry(
            pool: &SqlitePool,
            table: &str,
        ) -> Result<Option<Uuid>, sqlx::Error> {
            for _ in 0..5 {
                match claim_next_pending(pool, table).await {
                    Ok(result) => return Ok(result),
                    Err(sqlx::Error::Database(db_err))
                        if db_err.message().contains("database is locked") =>
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                    }
                    Err(err) => return Err(err),
                }
            }

            claim_next_pending(pool, table).await
        }

        let (claim_a, claim_b) = tokio::join!(
            claim_with_lock_retry(pool_a.as_ref(), "axon_crawl_jobs"),
            claim_with_lock_retry(pool_b.as_ref(), "axon_crawl_jobs")
        );

        let claims = [claim_a.expect("claim a"), claim_b.expect("claim b")];
        let winners = claims.iter().filter(|claim| **claim == Some(id)).count();
        assert_eq!(
            winners, 1,
            "exactly one worker should claim the pending job"
        );

        let status: (String,) = sqlx::query_as("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(pool_a.as_ref())
            .await
            .expect("fetch");
        assert_eq!(status.0, "running");

        drop(pool_a);
        drop(pool_b);
        tokio::fs::remove_file(&path).await.ok();
    }
}

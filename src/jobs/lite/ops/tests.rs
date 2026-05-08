use super::enqueue::check_pending_cap_for;
use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::lite::ops::{
    cancel_row, claim_next_pending, enqueue_job, mark_completed, mark_failed, update_result_json,
};
use crate::jobs::lite::store::open_sqlite_pool;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");

    let claimed = claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    assert_eq!(claimed, Some(id));
}

#[tokio::test]
async fn claim_returns_none_when_queue_empty() {
    let pool = test_pool().await;
    let claimed = claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    assert_eq!(claimed, None);
}

#[tokio::test]
async fn claim_clears_reclaimed_error_text() {
    let pool = test_pool().await;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_embed_jobs \
         (id, status, input_text, config_json, created_at, updated_at, error_text) \
         VALUES (?, 'pending', 'docs', '{}', 1, 1, 'reclaimed after unexpected shutdown')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .expect("insert reclaimed pending embed job");

    let claimed = claim_next_pending(&pool, JobKind::Embed)
        .await
        .expect("claim");
    assert_eq!(claimed.map(|id| id.to_string()), Some(id.clone()));

    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, error_text FROM axon_embed_jobs WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("claimed row");
    assert_eq!(row.0, "running");
    assert_eq!(row.1, None);
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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Embed)
        .await
        .expect("claim");
    mark_completed(&pool, JobKind::Embed, id, None)
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
async fn update_result_json_persists_progress_without_changing_status() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Ingest {
            target: "owner/repo".into(),
            source_type: "github".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Ingest)
        .await
        .expect("claim");

    update_result_json(
        &pool,
        JobKind::Ingest,
        id,
        &serde_json::json!({
            "phase": "collecting_files",
            "files_done": 25,
            "files_total": 100,
            "chunks_embedded": 42,
        }),
    )
    .await
    .expect("persist progress");

    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, result_json FROM axon_ingest_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
    assert_eq!(row.0, "running");
    let result_json: serde_json::Value =
        serde_json::from_str(&row.1.expect("result json")).expect("json");
    assert_eq!(result_json["phase"], "collecting_files");
    assert_eq!(result_json["files_done"], 25);
    assert_eq!(result_json["chunks_embedded"], 42);
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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    mark_failed(&pool, JobKind::Crawl, id, "connection timeout")
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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Embed)
        .await
        .expect("claim");
    sqlx::query("UPDATE axon_embed_jobs SET result_json=? WHERE id=?")
        .bind(r#"{"phase":"running"}"#)
        .bind(id.to_string())
        .execute(&pool)
        .await
        .expect("seed result");

    mark_completed(&pool, JobKind::Embed, id, None)
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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");

    let canceled = cancel_row(&pool, JobKind::Crawl, id).await.expect("cancel");
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
async fn cancel_row_returns_false_for_terminal_jobs() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    mark_completed(&pool, JobKind::Crawl, id, None)
        .await
        .expect("complete");

    let canceled = cancel_row(&pool, JobKind::Crawl, id).await.expect("cancel");
    assert!(
        !canceled,
        "cancel_row should return false for a completed job"
    );
}

#[tokio::test]
async fn mark_completed_succeeds_when_job_already_canceled() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");
    claim_next_pending(&pool, JobKind::Crawl)
        .await
        .expect("claim");
    cancel_row(&pool, JobKind::Crawl, id).await.expect("cancel");

    mark_completed(&pool, JobKind::Crawl, id, None)
        .await
        .expect("mark_completed should not error on canceled job");

    let status: (String,) = sqlx::query_as("SELECT status FROM axon_crawl_jobs WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .expect("fetch");
    assert_eq!(status.0, "canceled");
}

// ── Queue cap tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn embed_queue_cap_rejects_when_full() {
    let pool = test_pool().await;
    // Enqueue 2 jobs directly (bypassing cap check), then verify check_pending_cap_for
    // rejects at limit=2 and allows at limit=3.
    sqlx::query(
        "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
         VALUES ('id-1', 'pending', 'a', '{}', 0, 0), ('id-2', 'pending', 'b', '{}', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("seed rows");

    let err = check_pending_cap_for(&pool, "axon_embed_jobs", "embed", 2)
        .await
        .expect_err("should be at capacity");
    let msg = err.to_string();
    assert!(
        msg.contains("embed queue is at capacity"),
        "unexpected error message: {msg}"
    );

    // limit=3 allows one more
    check_pending_cap_for(&pool, "axon_embed_jobs", "embed", 3)
        .await
        .expect("limit=3 should allow 2 pending jobs");
}

#[tokio::test]
async fn extract_queue_cap_rejects_when_full() {
    let pool = test_pool().await;
    // Seed 1 pending extract job directly, then verify cap check at limit=1 rejects.
    sqlx::query(
        "INSERT INTO axon_extract_jobs (id, status, urls_json, config_json, created_at, updated_at) \
         VALUES ('id-1', 'pending', '[]', '{}', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("seed row");

    let err = check_pending_cap_for(&pool, "axon_extract_jobs", "extract", 1)
        .await
        .expect_err("should be at capacity");
    let msg = err.to_string();
    assert!(
        msg.contains("extract queue is at capacity"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn ingest_queue_cap_rejects_when_full() {
    let pool = test_pool().await;
    // Seed 1 pending ingest job directly, then verify cap check at limit=1 rejects.
    sqlx::query(
        "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
         VALUES ('id-1', 'pending', 'owner/repo', 'github', '{}', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("seed row");

    let err = check_pending_cap_for(&pool, "axon_ingest_jobs", "ingest", 1)
        .await
        .expect_err("should be at capacity");
    let msg = err.to_string();
    assert!(
        msg.contains("ingest queue is at capacity"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn embed_queue_cap_zero_disables_limit() {
    let pool = test_pool().await;
    // Setting limit=0 should allow any number of pending jobs.
    for i in 0..5 {
        enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: format!("item-{i}"),
                config_json: "{}".into(),
            },
            &Config::default_lite(),
        )
        .await
        .unwrap_or_else(|e| panic!("enqueue {i} failed: {e}"));
    }
    // With 5 pending jobs, limit=0 still allows more.
    check_pending_cap_for(&pool, "axon_embed_jobs", "embed", 0)
        .await
        .expect("limit=0 should be unlimited");
}

#[tokio::test]
async fn embed_queue_cap_allows_after_drain() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "first".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .expect("first enqueue");

    // Queue is at cap (1 pending, limit=1) — check should reject.
    check_pending_cap_for(&pool, "axon_embed_jobs", "embed", 1)
        .await
        .expect_err("should be at capacity");

    // Drain: claim + complete the pending job.
    claim_next_pending(&pool, JobKind::Embed)
        .await
        .expect("claim");
    mark_completed(&pool, JobKind::Embed, id, None)
        .await
        .expect("complete");

    // Now the cap check should pass (0 pending, limit=1).
    check_pending_cap_for(&pool, "axon_embed_jobs", "embed", 1)
        .await
        .expect("cap check after drain should succeed");
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
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");

    async fn claim_with_lock_retry(
        pool: &SqlitePool,
        kind: JobKind,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        for _ in 0..5 {
            match claim_next_pending(pool, kind).await {
                Ok(result) => return Ok(result),
                Err(sqlx::Error::Database(db_err))
                    if db_err.message().contains("database is locked") =>
                {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(err) => return Err(err),
            }
        }
        claim_next_pending(pool, kind).await
    }

    let (claim_a, claim_b) = tokio::join!(
        claim_with_lock_retry(pool_a.as_ref(), JobKind::Crawl),
        claim_with_lock_retry(pool_b.as_ref(), JobKind::Crawl)
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

#[tokio::test]
async fn touch_heartbeat_advances_updated_at_only_on_running_rows() {
    use crate::jobs::lite::ops::touch_heartbeat;
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "test".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .expect("enqueue");

    // Pending rows are not heartbeated.
    let before_pending: (i64,) =
        sqlx::query_as("SELECT updated_at FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
    tokio::time::sleep(Duration::from_millis(5)).await;
    touch_heartbeat(&pool, JobKind::Embed, id)
        .await
        .expect("touch");
    let after_pending: (i64,) =
        sqlx::query_as("SELECT updated_at FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
    assert_eq!(
        before_pending.0, after_pending.0,
        "touch_heartbeat must not bump pending rows"
    );

    // Claim the job — now in running state.
    claim_next_pending(&pool, JobKind::Embed)
        .await
        .expect("claim");
    let before_running: (i64,) =
        sqlx::query_as("SELECT updated_at FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
    tokio::time::sleep(Duration::from_millis(5)).await;
    touch_heartbeat(&pool, JobKind::Embed, id)
        .await
        .expect("touch");
    let after_running: (i64,) =
        sqlx::query_as("SELECT updated_at FROM axon_embed_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch");
    assert!(
        after_running.0 > before_running.0,
        "touch_heartbeat must advance updated_at on running rows"
    );
}

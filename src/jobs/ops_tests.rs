use super::enqueue::check_pending_cap_for;
use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::error::JobError;
use crate::jobs::ops::{
    cancel_row, claim_next_pending, claim_next_pending_for_attempt, enqueue_job, mark_completed,
    mark_completed_for_attempt, mark_failed, touch_heartbeat_for_attempt, update_result_json,
    update_result_json_for_attempt,
};
use crate::jobs::store::{
    RECLAIMED_ERROR_TEXT, open_sqlite_pool, reclaim_stale_running_jobs_for_table,
};
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
        &Config::default_minimal(),
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
async fn claim_preserves_reclaimed_error_text_until_terminal_state() {
    let pool = test_pool().await;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_embed_jobs \
         (id, status, input_text, config_json, created_at, updated_at, error_text) \
         VALUES (?, 'pending', 'docs', '{}', 1, 1, ?)",
    )
    .bind(&id)
    .bind(RECLAIMED_ERROR_TEXT)
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
    assert_eq!(row.1.as_deref(), Some(RECLAIMED_ERROR_TEXT));
}

#[tokio::test]
async fn claim_assigns_attempt_metadata_and_reclaim_creates_new_attempt() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "docs".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");

    let first = claim_next_pending_for_attempt(&pool, JobKind::Embed)
        .await
        .expect("claim")
        .expect("claimed");
    assert_eq!(first.id, id);
    assert_eq!(first.attempt_count, 1);

    sqlx::query("UPDATE axon_embed_jobs SET updated_at = 1 WHERE id = ?")
        .bind(id.to_string())
        .execute(&pool)
        .await
        .expect("age row");
    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Embed, 5_000)
        .await
        .expect("reclaim");
    assert_eq!(reclaimed, 1);

    let second = claim_next_pending_for_attempt(&pool, JobKind::Embed)
        .await
        .expect("second claim")
        .expect("claimed");
    assert_eq!(second.id, id);
    assert_eq!(second.attempt_count, 2);
    assert_ne!(first.attempt_id, second.attempt_id);
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
        &Config::default_minimal(),
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
async fn stale_attempt_writes_are_rejected_after_reclaim_and_retry() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");

    let first = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("claim")
        .expect("claimed");
    sqlx::query("UPDATE axon_crawl_jobs SET updated_at = 1 WHERE id = ?")
        .bind(id.to_string())
        .execute(&pool)
        .await
        .expect("age row");
    reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000)
        .await
        .expect("reclaim");
    let second = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("retry claim")
        .expect("claimed");

    update_result_json_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&first.attempt_id),
        &serde_json::json!({ "pages_crawled": 999 }),
    )
    .await
    .expect("stale progress ignored");
    touch_heartbeat_for_attempt(&pool, JobKind::Crawl, id, Some(&first.attempt_id))
        .await
        .expect("stale heartbeat ignored");
    mark_completed_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&first.attempt_id),
        Some(&serde_json::json!({ "stale": true })),
    )
    .await
    .expect("stale complete ignored");

    let row: (String, Option<String>, i64) = sqlx::query_as(
        "SELECT status, result_json, attempt_count FROM axon_crawl_jobs WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(row.0, "running");
    assert_eq!(row.1, None);
    assert_eq!(row.2, 2);

    update_result_json_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&second.attempt_id),
        &serde_json::json!({ "pages_crawled": 1 }),
    )
    .await
    .expect("current progress accepted");
    let progress_json: Option<String> =
        sqlx::query_scalar("SELECT progress_json FROM axon_crawl_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("progress json");
    assert!(
        progress_json
            .as_deref()
            .is_some_and(|json| json.contains("pages_crawled")),
        "current attempt should be able to persist progress"
    );
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
        &Config::default_minimal(),
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

    let row: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, progress_json, result_json FROM axon_ingest_jobs WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .expect("fetch");
    assert_eq!(row.0, "running");
    let progress_json: serde_json::Value =
        serde_json::from_str(&row.1.expect("progress json")).expect("json");
    assert_eq!(
        row.2, None,
        "live progress must not write final result_json"
    );
    assert_eq!(progress_json["phase"], "collecting_files");
    assert_eq!(progress_json["files_done"], 25);
    assert_eq!(progress_json["chunks_embedded"], 42);
}

#[tokio::test]
async fn progress_updates_are_separate_from_final_result_json() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");
    let attempt = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("claim")
        .expect("claimed");

    update_result_json_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&attempt.attempt_id),
        &serde_json::json!({
            "phase": "crawling",
            "lifecycle_progress": 0.42,
            "pages_crawled": 42,
            "pages_discovered": 100
        }),
    )
    .await
    .expect("persist progress");

    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT progress_json, result_json FROM axon_crawl_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("row");
    assert!(
        row.0
            .as_deref()
            .is_some_and(|json| json.contains("lifecycle_progress"))
    );
    assert_eq!(
        row.1, None,
        "live progress must not pollute final result_json"
    );

    mark_completed_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&attempt.attempt_id),
        Some(&serde_json::json!({
            "url": "https://example.com",
            "coverage_status": "partial",
            "coverage_summary": "max pages hit",
            "pages_crawled": 42
        })),
    )
    .await
    .expect("complete");

    let row: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, progress_json, result_json FROM axon_crawl_jobs WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(row.0, "completed");
    let progress: serde_json::Value =
        serde_json::from_str(row.1.as_deref().expect("progress json")).expect("progress json");
    let result: serde_json::Value =
        serde_json::from_str(row.2.as_deref().expect("result json")).expect("result json");
    assert_eq!(progress["lifecycle_progress"], serde_json::json!(1.0));
    assert_eq!(progress["phase"], serde_json::json!("completed"));
    assert_eq!(result["lifecycle_progress"], serde_json::Value::Null);
    assert_eq!(result["coverage_status"], "partial");
}

#[tokio::test]
async fn reclaim_marks_progress_json_requeued_and_keeps_previous_attempt_progress() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");
    let attempt = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("claim")
        .expect("claimed");
    update_result_json_for_attempt(
        &pool,
        JobKind::Crawl,
        id,
        Some(&attempt.attempt_id),
        &serde_json::json!({
            "phase": "crawling",
            "lifecycle_progress": 0.33,
            "pages_crawled": 33
        }),
    )
    .await
    .expect("progress");
    sqlx::query("UPDATE axon_crawl_jobs SET updated_at = 1 WHERE id = ?")
        .bind(id.to_string())
        .execute(&pool)
        .await
        .expect("age row");

    reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000)
        .await
        .expect("reclaim");

    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, progress_json FROM axon_crawl_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("row");
    assert_eq!(row.0, "pending");
    let progress: serde_json::Value =
        serde_json::from_str(row.1.as_deref().expect("progress json")).expect("progress json");
    assert_eq!(progress["phase"], "requeued");
    assert_eq!(progress["lifecycle_progress"], serde_json::json!(0.0));
    assert_eq!(progress["previous_attempt_progress"]["pages_crawled"], 33);
}

#[tokio::test]
async fn update_result_json_skips_non_running_rows() {
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "test".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");

    update_result_json(
        &pool,
        JobKind::Embed,
        id,
        &serde_json::json!({ "chunks_embedded": 99 }),
    )
    .await
    .expect("skip pending progress");

    let row: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, progress_json, result_json FROM axon_embed_jobs WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .expect("fetch");
    assert_eq!(row.0, "pending");
    assert_eq!(row.1, None);
    assert_eq!(row.2, None);
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
        &Config::default_minimal(),
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
        &Config::default_minimal(),
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
        &Config::default_minimal(),
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
        &Config::default_minimal(),
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
        &Config::default_minimal(),
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

    let err = check_pending_cap_for(&pool, JobKind::Embed, 2)
        .await
        .expect_err("should be at capacity");
    let msg = err.to_string();
    assert!(
        msg.contains("embed queue is at capacity"),
        "unexpected error message: {msg}"
    );

    // limit=3 allows one more
    check_pending_cap_for(&pool, JobKind::Embed, 3)
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

    let err = check_pending_cap_for(&pool, JobKind::Extract, 1)
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

    let err = check_pending_cap_for(&pool, JobKind::Ingest, 1)
        .await
        .expect_err("should be at capacity");
    let msg = err.to_string();
    assert!(
        msg.contains("ingest queue is at capacity"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn ingest_claim_serializes_same_target_jobs() {
    // Two jobs for the same (source_type, target) must never run concurrently:
    // the first job's repo-scoped stale cleanup would delete points the second
    // just upserted. A pending job whose target has a running sibling is left
    // queued; jobs for other targets are claimed past it.
    let pool = test_pool().await;
    let dup_1 = "00000000-0000-0000-0000-000000000001";
    let dup_2 = "00000000-0000-0000-0000-000000000002";
    let other = "00000000-0000-0000-0000-000000000003";
    for (id, created_at, source_type, target) in [
        (dup_1, 0, "github", "owner/repo"),
        (dup_2, 1, "github", "owner/repo"),
        (other, 2, "reddit", "r/rust"),
    ] {
        sqlx::query(
            "INSERT INTO axon_ingest_jobs (id, status, target, source_type, config_json, created_at, updated_at) \
             VALUES (?, 'pending', ?, ?, '{}', ?, ?)",
        )
        .bind(id)
        .bind(target)
        .bind(source_type)
        .bind(created_at)
        .bind(created_at)
        .execute(&pool)
        .await
        .expect("seed row");
    }

    let claimed_ids = |pool: &SqlitePool| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM axon_ingest_jobs WHERE status='running' ORDER BY id",
            )
            .fetch_all(&pool)
            .await
            .expect("running ids")
        }
    };

    // First claim: oldest pending job (dup_1) starts running.
    claim_next_pending(&pool, JobKind::Ingest)
        .await
        .expect("claim 1")
        .expect("dup_1 claimable");
    assert_eq!(claimed_ids(&pool).await, vec![dup_1.to_string()]);

    // Second claim must skip dup_2 (same target as the running dup_1) and
    // claim the other-target job instead.
    claim_next_pending(&pool, JobKind::Ingest)
        .await
        .expect("claim 2")
        .expect("other-target job claimable");
    assert_eq!(
        claimed_ids(&pool).await,
        vec![dup_1.to_string(), other.to_string()]
    );

    // Nothing left that is safe to claim while dup_1 runs.
    assert_eq!(
        claim_next_pending(&pool, JobKind::Ingest)
            .await
            .expect("claim 3"),
        None
    );

    // Once dup_1 reaches a terminal state, dup_2 becomes claimable.
    sqlx::query("UPDATE axon_ingest_jobs SET status='completed' WHERE id=?")
        .bind(dup_1)
        .execute(&pool)
        .await
        .expect("complete dup_1");
    let claimed = claim_next_pending(&pool, JobKind::Ingest)
        .await
        .expect("claim 4")
        .expect("dup_2 claimable after sibling completes");
    assert_eq!(claimed.to_string(), dup_2);
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
            &Config::default_minimal(),
        )
        .await
        .unwrap_or_else(|e| panic!("enqueue {i} failed: {e}"));
    }
    // With 5 pending jobs, limit=0 still allows more.
    check_pending_cap_for(&pool, JobKind::Embed, 0)
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
        &Config::default_minimal(),
    )
    .await
    .expect("first enqueue");

    // Queue is at cap (1 pending, limit=1) — check should reject.
    check_pending_cap_for(&pool, JobKind::Embed, 1)
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
    check_pending_cap_for(&pool, JobKind::Embed, 1)
        .await
        .expect("cap check after drain should succeed");
}

#[tokio::test]
async fn concurrent_claims_only_return_one_job() {
    let path = std::env::temp_dir()
        .join(format!("axon-sqlite-claim-{}.db", Uuid::new_v4()))
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
        &Config::default_minimal(),
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
async fn enqueue_retries_when_sqlite_write_lock_is_temporarily_held() {
    let path = std::env::temp_dir()
        .join(format!("axon-sqlite-enqueue-lock-{}.db", Uuid::new_v4()))
        .to_string_lossy()
        .into_owned();
    let pool_a = open_sqlite_pool(&path).await.expect("pool a");
    let pool_b = open_sqlite_pool(&path).await.expect("pool b");
    let mut lock_conn = pool_a.acquire().await.expect("lock conn");

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *lock_conn)
        .await
        .expect("hold write lock");

    // 1.5s hold exercises the retry/wait path while staying well inside the
    // 10s busy_timeout configured in open_sqlite_pool. The original 5.2s was
    // chosen to outwait the prior 5s timeout — no longer needed.
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        sqlx::query("ROLLBACK")
            .execute(&mut *lock_conn)
            .await
            .expect("release write lock");
    });

    let id = enqueue_job(
        &pool_b,
        &JobPayload::Crawl {
            url: "https://locked.example".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue should wait/retry until the write lock clears");

    release.await.expect("release task");
    let row: (String,) = sqlx::query_as("SELECT url FROM axon_crawl_jobs WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&pool_b)
        .await
        .expect("fetch inserted row");
    assert_eq!(row.0, "https://locked.example");

    drop(pool_a);
    drop(pool_b);
    tokio::fs::remove_file(&path).await.ok();
}

#[tokio::test]
async fn concurrent_enqueue_respects_pending_cap_atomically() {
    let path = std::env::temp_dir()
        .join(format!("axon-sqlite-enqueue-cap-{}.db", Uuid::new_v4()))
        .to_string_lossy()
        .into_owned();
    let pool_a = Arc::new(open_sqlite_pool(&path).await.expect("pool a"));
    let pool_b = Arc::new(open_sqlite_pool(&path).await.expect("pool b"));
    let mut cfg = Config::default_minimal();
    cfg.max_pending_crawl_jobs = 1;

    let cfg_a = cfg.clone();
    let cfg_b = cfg;
    let first = {
        let pool = Arc::clone(&pool_a);
        tokio::spawn(async move {
            enqueue_job(
                pool.as_ref(),
                &JobPayload::Crawl {
                    url: "https://cap-a.example".into(),
                    config_json: "{}".into(),
                },
                &cfg_a,
            )
            .await
        })
    };
    let second = {
        let pool = Arc::clone(&pool_b);
        tokio::spawn(async move {
            enqueue_job(
                pool.as_ref(),
                &JobPayload::Crawl {
                    url: "https://cap-b.example".into(),
                    config_json: "{}".into(),
                },
                &cfg_b,
            )
            .await
        })
    };

    let results = [
        first.await.expect("first join"),
        second.await.expect("second join"),
    ];
    let successes = results.iter().filter(|result| result.is_ok()).count();
    let cap_rejections = results
        .iter()
        .filter(|result| matches!(result, Err(JobError::QueueCapacityExceeded { .. })))
        .count();
    assert_eq!(successes, 1, "exactly one enqueue should fit cap=1");
    assert_eq!(
        cap_rejections, 1,
        "the other enqueue should see the serialized pending count"
    );

    let pending: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM axon_crawl_jobs WHERE status='pending'")
            .fetch_one(pool_a.as_ref())
            .await
            .expect("pending count");
    assert_eq!(pending.0, 1);

    drop(pool_a);
    drop(pool_b);
    tokio::fs::remove_file(&path).await.ok();
}

#[tokio::test]
async fn touch_heartbeat_advances_updated_at_only_on_running_rows() {
    use crate::jobs::ops::touch_heartbeat;
    let pool = test_pool().await;
    let id = enqueue_job(
        &pool,
        &JobPayload::Embed {
            input: "test".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
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

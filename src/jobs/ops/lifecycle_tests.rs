use super::*;
use crate::jobs::store::{now_ms, rollback_on_release};
use sqlx::sqlite::SqlitePoolOptions;

/// Build a single-connection in-memory pool that carries the production
/// `after_release` ROLLBACK hook, then create a minimal `axon_crawl_jobs`
/// table with every column `claim_next_pending_for_attempt_inner` reads or
/// writes. A single slot makes transaction-leak / best-effort-rollback
/// behavior deterministic: any dangling transaction immediately blocks the
/// next checkout's `BEGIN IMMEDIATE`.
async fn single_slot_pool_with_crawl_table() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect(":memory:")
        .await
        .expect("pool");

    sqlx::query(
        "CREATE TABLE axon_crawl_jobs (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            url TEXT,
            config_json TEXT,
            error_text TEXT,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            active_attempt_id TEXT,
            progress_json TEXT,
            result_json TEXT,
            started_at INTEGER,
            updated_at INTEGER,
            finished_at INTEGER,
            created_at INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .expect("create axon_crawl_jobs");

    pool
}

async fn insert_pending_crawl(pool: &SqlitePool, id: &str) {
    let now = now_ms();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://example.com', '{}', ?, ?)",
    )
    .bind(id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert pending crawl");
}

/// The None path (empty queue) issues a ROLLBACK on the manual transaction.
/// On a single-slot pool, if that ROLLBACK poisoned the slot the next claim
/// would fail to `BEGIN IMMEDIATE`. Run several empty-queue claims and then a
/// real claim to prove the slot stays usable across the best-effort ROLLBACK.
#[tokio::test]
async fn none_path_rollback_does_not_poison_single_slot() {
    let pool = single_slot_pool_with_crawl_table().await;

    // Empty queue → None path → best-effort ROLLBACK, repeated.
    for _ in 0..3 {
        let claimed = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
            .await
            .expect("claim on empty queue must succeed");
        assert!(claimed.is_none(), "empty queue should yield None");
    }

    // The single slot must still be transaction-free: a real claim must work.
    let id = Uuid::new_v4().to_string();
    insert_pending_crawl(&pool, &id).await;

    let claimed = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("claim must succeed after empty-queue rollbacks")
        .expect("the pending job should be claimed");
    assert_eq!(claimed.id.to_string(), id);
    assert_eq!(claimed.attempt_count, 1);

    let status: String = sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(
        status, "running",
        "claimed job must be committed as running"
    );
}

/// The success path commits via `ImmediateTx::commit`. Verify the commit is
/// durable (status persisted) AND the single slot is left transaction-free for
/// the next claim — i.e. neither the COMMIT nor a follow-up claim is wedged in
/// a dangling transaction.
#[tokio::test]
async fn success_path_commit_leaves_slot_clean() {
    let pool = single_slot_pool_with_crawl_table().await;

    let first = Uuid::new_v4().to_string();
    let second = Uuid::new_v4().to_string();
    insert_pending_crawl(&pool, &first).await;
    insert_pending_crawl(&pool, &second).await;

    let claimed_first = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("first claim")
        .expect("first job");

    // A second claim on the same single slot only works if the first claim's
    // COMMIT released the connection cleanly (no dangling tx).
    let claimed_second = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("second claim")
        .expect("second job");

    assert_ne!(claimed_first.id, claimed_second.id);

    let running: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs WHERE status = 'running'")
            .fetch_one(&pool)
            .await
            .expect("count running");
    assert_eq!(
        running, 2,
        "both committed claims must be persisted running"
    );
}

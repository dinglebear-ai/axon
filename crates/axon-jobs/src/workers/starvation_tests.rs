use super::*;

use crate::store::open_sqlite_pool;

async fn insert_interactive_job_at(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    created_at: &str,
    updated_at: &str,
) {
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, created_at, updated_at)
         VALUES (?, 'source', ?, 'queued', 'interactive', ?, ?)",
    )
    .bind(id)
    .bind(status)
    .bind(created_at)
    .bind(updated_at)
    .execute(pool)
    .await
    .expect("insert interactive job");
}

async fn insert_interactive_job(pool: &SqlitePool, id: &str, status: &str, timestamp: &str) {
    insert_interactive_job_at(pool, id, status, timestamp, timestamp).await;
}

#[tokio::test]
async fn disabled_slo_skips_detection() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let old = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    insert_interactive_job(&pool, "disabled", "queued", &old).await;

    assert!(!detect_interactive_starvation(&pool, 0).await);
}

#[tokio::test]
async fn running_interactive_job_prevents_starvation() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let old = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    insert_interactive_job(&pool, "queued", "queued", &old).await;
    insert_interactive_job(&pool, "running", "running", &old).await;

    assert!(!detect_interactive_starvation(&pool, 1).await);
}

#[tokio::test]
async fn queued_job_below_slo_is_not_starved() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let recent = chrono::Utc::now().to_rfc3339();
    insert_interactive_job(&pool, "recent", "queued", &recent).await;

    assert!(!detect_interactive_starvation(&pool, 60_000).await);
}

#[tokio::test]
async fn queued_job_beyond_slo_is_starved() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let old = (chrono::Utc::now() - chrono::Duration::seconds(2)).to_rfc3339();
    insert_interactive_job(&pool, "starved", "queued", &old).await;

    assert!(detect_interactive_starvation(&pool, 1_000).await);
}

#[tokio::test]
async fn retried_old_job_uses_recent_queue_entry_time() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let old = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let recent = chrono::Utc::now().to_rfc3339();
    insert_interactive_job_at(&pool, "retried", "queued", &old, &recent).await;

    assert!(!detect_interactive_starvation(&pool, 60_000).await);
}

#[tokio::test]
async fn malformed_queue_timestamp_is_not_reported_as_starvation() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    insert_interactive_job(&pool, "malformed", "queued", "not-a-timestamp").await;

    assert!(!detect_interactive_starvation(&pool, 1).await);
}

use super::*;
use uuid::Uuid;

#[tokio::test]
async fn reclaim_stale_running_jobs_only_reclaims_stale_running_rows() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_id = Uuid::new_v4().to_string();
    let fresh_id = Uuid::new_v4().to_string();
    let pending_id = Uuid::new_v4().to_string();
    let stale_updated_at = now_ms() - 10_000;
    let fresh_updated_at = now_ms();

    for (id, status, updated_at) in [
        (&stale_id, "running", stale_updated_at),
        (&fresh_id, "running", fresh_updated_at),
        (&pending_id, "pending", stale_updated_at),
    ] {
        sqlx::query(
            "INSERT INTO axon_embed_jobs (id, status, input_text, config_json, created_at, updated_at) \
             VALUES (?, ?, ?, '{}', ?, ?)",
        )
        .bind(id)
        .bind(status)
        .bind("test input")
        .bind(updated_at)
        .bind(updated_at)
        .execute(&pool)
        .await
        .expect("insert job");
    }

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Embed, 5_000)
        .await
        .expect("reclaim");

    assert_eq!(reclaimed, 1);
    let stale_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&stale_id)
            .fetch_one(&pool)
            .await
            .expect("stale status");
    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&fresh_id)
            .fetch_one(&pool)
            .await
            .expect("fresh status");
    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM axon_embed_jobs WHERE id = ?")
            .bind(&pending_id)
            .fetch_one(&pool)
            .await
            .expect("pending status");

    assert_eq!(stale_status, "pending");
    assert_eq!(fresh_status, "running");
    assert_eq!(pending_status, "pending");
}

#[tokio::test]
async fn reclaim_stale_running_jobs_for_table_sets_reclaim_error_text() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_updated_at = now_ms() - 10_000;

    let stale_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at) \
         VALUES (?, 'running', 'https://stale.example', '{}', ?, ?, ?)",
    )
    .bind(&stale_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let fresh_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'running', 'https://fresh.example', '{}', ?, ?)",
    )
    .bind(&fresh_id)
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await
    .unwrap();

    let pending_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
    )
    .bind(&pending_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000)
        .await
        .expect("reclaim");

    assert_eq!(
        reclaimed, 1,
        "only the stale running row should be reclaimed"
    );

    let (status, error_text, active_attempt_id, last_reclaimed_at): (
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT status, error_text, active_attempt_id, last_reclaimed_at \
             FROM axon_crawl_jobs WHERE id = ?",
    )
    .bind(&stale_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(error_text.as_deref(), Some(RECLAIMED_ERROR_TEXT));
    assert_eq!(active_attempt_id, None);
    assert!(last_reclaimed_at.is_some());

    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&fresh_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fresh_status, "running", "fresh row must not be reclaimed");

    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&pending_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending_status, "pending", "pending row must not be touched");
}

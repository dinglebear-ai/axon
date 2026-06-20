use super::*;
use uuid::Uuid;

#[tokio::test]
async fn migration_0014_moves_only_active_result_json_to_progress_json() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("pool");

    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        sqlx::query(&format!(
            "CREATE TABLE {table} (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                result_json TEXT
            )"
        ))
        .execute(&pool)
        .await
        .expect("create pre-0013 table");

        sqlx::query(&format!(
            "INSERT INTO {table} (id, status, result_json) VALUES ('active', 'running', ?)"
        ))
        .bind(r#"{"lifecycle_progress":0.7,"pages_crawled":14}"#)
        .execute(&pool)
        .await
        .expect("insert active row");

        sqlx::query(&format!(
            "INSERT INTO {table} (id, status, result_json) VALUES ('done', 'completed', ?)"
        ))
        .bind(r#"{"pages_crawled":20,"coverage_status":"complete"}"#)
        .execute(&pool)
        .await
        .expect("insert completed row");
    }

    for migration in [
        include_str!("migrations/0013_add_job_progress_json.sql"),
        include_str!("migrations/0014_backfill_active_job_progress_json.sql"),
    ] {
        for statement in migration
            .split(';')
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
        {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("run migration statement");
        }
    }

    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        let (active_progress, active_result): (Option<String>, Option<String>) = sqlx::query_as(
            &format!("SELECT progress_json, result_json FROM {table} WHERE id = 'active'"),
        )
        .fetch_one(&pool)
        .await
        .expect("active row");
        assert_eq!(
            active_progress.as_deref(),
            Some(r#"{"lifecycle_progress":0.7,"pages_crawled":14}"#),
            "{table} should preserve active progress"
        );
        assert_eq!(
            active_result, None,
            "{table} should clear active terminal result"
        );

        let (done_progress, done_result): (Option<String>, Option<String>) = sqlx::query_as(
            &format!("SELECT progress_json, result_json FROM {table} WHERE id = 'done'"),
        )
        .fetch_one(&pool)
        .await
        .expect("completed row");
        assert_eq!(done_progress, None, "{table} should not invent progress");
        assert_eq!(
            done_result.as_deref(),
            Some(r#"{"pages_crawled":20,"coverage_status":"complete"}"#),
            "{table} should preserve terminal result"
        );
    }
}

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

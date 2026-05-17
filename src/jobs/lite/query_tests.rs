use super::*;
use crate::core::config::Config;
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
        &Config::default_minimal(),
    )
    .await
    .unwrap();
    enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://b.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .unwrap();

    let jobs = list_jobs(&pool, JobKind::Crawl).await.unwrap();
    assert_eq!(jobs.len(), 2);
    // Both jobs have the same created_at (tight loop), so order is by insertion
    // Either order is acceptable; just check both exist
    let targets: std::collections::HashSet<_> = jobs.iter().map(|j| j.target.as_str()).collect();
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
async fn list_service_jobs_prioritizes_running_crawl_rows_over_newer_pending_rows() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let older_running = Uuid::new_v4().to_string();
    let newer_pending = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at, started_at) \
         VALUES (?, 'running', 'https://running.example', '{}', ?, ?, ?)",
    )
    .bind(&older_running)
    .bind(1_000_i64)
    .bind(1_000_i64)
    .bind(1_000_i64)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
    )
    .bind(&newer_pending)
    .bind(2_000_i64)
    .bind(2_000_i64)
    .execute(&pool)
    .await
    .unwrap();

    let jobs = list_service_jobs(&pool, JobKind::Crawl, 20, 0)
        .await
        .unwrap();

    assert_eq!(jobs[0].id.to_string(), older_running);
    assert_eq!(jobs[0].status, "running");
    assert_eq!(jobs[1].id.to_string(), newer_pending);
    assert_eq!(jobs[1].status, "pending");
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

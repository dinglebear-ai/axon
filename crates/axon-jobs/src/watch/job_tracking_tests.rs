use super::*;
use axon_api::source::{JobKind, JobListRequest, LifecycleStatus};
use std::error::Error;

use crate::store::{now_ms, open_sqlite_pool};
use crate::unified::SqliteUnifiedJobStore;

#[tokio::test]
async fn start_and_finish_watch_job_completed_is_queryable_by_kind() -> Result<(), Box<dyn Error>> {
    let pool = open_sqlite_pool(":memory:").await?;
    let watch_id = Uuid::new_v4();
    // Satisfy jobs.watch_id's FK to axon_watch_defs(id).
    sqlx::query(
        "INSERT INTO axon_watch_defs (
            id, name, task_type, task_payload, every_seconds, enabled,
            next_run_at, created_at, updated_at
        ) VALUES (?, 'tracking-test', 'watch', '{}', 60, 1, ?, ?, ?)",
    )
    .bind(watch_id.to_string())
    .bind(now_ms())
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await?;

    let job_id = start_watch_job(&pool, watch_id)
        .await
        .expect("mirror job creation should succeed against a fresh schema");

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let running = store
        .get(job_id)
        .await
        .expect("get should not error")
        .expect("job row should exist after start_watch_job");
    assert_eq!(running.kind, JobKind::Watch);
    assert_eq!(running.status, LifecycleStatus::Running);
    assert_eq!(
        running.watch_id.as_ref().map(|w| w.0.clone()),
        Some(watch_id.to_string())
    );

    finish_watch_job(&pool, Some(job_id), Ok(())).await;

    let completed = store
        .get(job_id)
        .await
        .expect("get should not error")
        .expect("job row should still exist after finish");
    assert_eq!(completed.status, LifecycleStatus::Completed);

    // Queryable via the same `--kind watch` filter path `axon jobs list` uses.
    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Watch),
            source_id: None,
            watch_id: None,
            limit: None,
            cursor: None,
        })
        .await
        .expect("list should not error");
    assert!(
        page.items.iter().any(|item| item.job_id == job_id),
        "watch job must be visible under kind=watch listing"
    );

    Ok(())
}

#[tokio::test]
async fn finish_watch_job_marks_failed_from_real_error_message() -> Result<(), Box<dyn Error>> {
    let pool = open_sqlite_pool(":memory:").await?;
    let watch_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO axon_watch_defs (
            id, name, task_type, task_payload, every_seconds, enabled,
            next_run_at, created_at, updated_at
        ) VALUES (?, 'tracking-failure', 'watch', '{}', 60, 1, ?, ?, ?)",
    )
    .bind(watch_id.to_string())
    .bind(now_ms())
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await?;

    let job_id = start_watch_job(&pool, watch_id).await.expect("start");
    finish_watch_job(&pool, Some(job_id), Err("boom: upstream timed out")).await;

    let store = SqliteUnifiedJobStore::new(pool.clone());
    let failed = store
        .get(job_id)
        .await
        .expect("get should not error")
        .expect("job row should exist");
    assert_eq!(failed.status, LifecycleStatus::Failed);
    assert_eq!(
        failed.last_error.as_ref().map(|e| e.message.as_str()),
        Some("boom: upstream timed out")
    );
    Ok(())
}

#[tokio::test]
async fn finish_watch_job_with_none_job_id_is_a_no_op() -> Result<(), Box<dyn Error>> {
    // A `None` job_id models mirror-creation failure: finish must not panic
    // or touch the store, since there is nothing to finalize.
    let pool = open_sqlite_pool(":memory:").await?;
    finish_watch_job(&pool, None, Ok(())).await;
    Ok(())
}

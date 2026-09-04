use super::*;

#[tokio::test]
async fn pending_job_counter_reads_the_canonical_job_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jobs.db");
    let pool = open_sqlite_pool(path.to_string_lossy().as_ref())
        .await
        .expect("job store");
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, intent, status, phase, attempt, priority, auth_snapshot_json, created_at, updated_at) VALUES (?, 'source', 'run', 'queued', 'queued', 1, 'normal', '{}', ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await
    .expect("queued job");
    drop(pool);

    assert_eq!(count_pending_jobs(&path).await.expect("count"), 1);
}

#[tokio::test]
async fn quick_check_reports_clean_database() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory SQLite pool");

    assert!(
        quick_check_is_clean(&pool).await.expect("quick_check runs"),
        "a freshly opened SQLite database must report ok"
    );
}

#[test]
fn failed_integrity_probe_does_not_advance_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    let db_path = db_path.to_string_lossy();

    assert!(should_run_integrity_probe(&db_path));
    assert!(
        !finish_integrity_probe(
            &db_path,
            Err(sqlx::Error::Protocol(
                "injected quick_check failure".to_string()
            ))
        ),
        "an unavailable probe is not evidence of corruption"
    );
    assert!(
        should_run_integrity_probe(&db_path),
        "an unavailable probe must not suppress the next integrity check"
    );
}

#[test]
fn clean_integrity_probe_advances_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("jobs.db");
    let db_path = db_path.to_string_lossy();

    assert!(!finish_integrity_probe(&db_path, Ok(true)));
    assert!(!should_run_integrity_probe(&db_path));
}

use super::*;

#[tokio::test]
async fn incompatible_old_store_blocks_workers_before_runtime_side_effects() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("jobs.db");
    let url = format!("sqlite://{}?mode=rwc", db.display());
    let pool = sqlx::SqlitePool::connect(&url).await.expect("open sqlite");
    sqlx::query("CREATE TABLE axon_crawl_jobs (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create incompatible table");
    sqlx::query("INSERT INTO axon_crawl_jobs (id) VALUES ('old-job')")
        .execute(&pool)
        .await
        .expect("seed incompatible row");
    pool.close().await;

    let mut cfg = Config::test_default();
    cfg.sqlite_path = db.clone();
    cfg.qdrant_url.clear();
    let error = match resolve_runtime_with_workers(Arc::new(cfg), true).await {
        Ok(_) => panic!("old store must block workers"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("startup.incompatible_store"));

    let ro = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", db.display()))
        .await
        .expect("reopen sqlite");
    let jobs_table: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='jobs'")
            .fetch_one(&ro)
            .await
            .expect("probe jobs table");
    ro.close().await;
    assert_eq!(
        jobs_table, 0,
        "runtime must not migrate before blocker check"
    );
}

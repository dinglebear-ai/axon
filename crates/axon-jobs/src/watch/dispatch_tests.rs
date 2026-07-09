use super::*;
use crate::store::open_sqlite_pool;
use axon_core::config::Config;
use tempfile::NamedTempFile;

fn test_cfg(path: &std::path::Path) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = path.to_path_buf();
    cfg
}

#[tokio::test]
async fn pending_crawl_is_active_unknown_is_not() {
    let temp = NamedTempFile::new().unwrap();
    let cfg = test_cfg(temp.path());
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let id = enqueue_change_crawl(&pool, &cfg, "https://e/a/", 2)
        .await
        .unwrap();
    assert!(crawl_job_active(&pool, id).await);
    assert!(!crawl_job_active(&pool, Uuid::new_v4()).await);
}

/// Regression test for the bug where `enqueue_change_crawl` wrote to the
/// legacy `axon_crawl_jobs` table, which nothing claims anymore after
/// `ca7ea71d1` retired the legacy crawl worker lane. This drives a
/// watch-triggered crawl dispatch end-to-end and asserts the resulting job
/// actually transitions out of `pending` (i.e. it's claimable by the unified
/// worker), not just that the enqueue call returns `Ok`.
#[tokio::test]
async fn enqueued_change_crawl_is_claimable_by_unified_worker() {
    let temp = NamedTempFile::new().unwrap();
    let cfg = test_cfg(temp.path());
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();

    let job_id = enqueue_change_crawl(&pool, &cfg, "https://example.com/docs/", 3)
        .await
        .unwrap();

    // The job must exist in the unified `jobs` table (not the legacy
    // `axon_crawl_jobs` table) with a status a worker can claim.
    let store = SqliteUnifiedJobStore::new(pool.clone());
    let summary = store
        .get(JobId(job_id))
        .await
        .unwrap()
        .expect("job must exist in the unified job store");
    assert_eq!(summary.kind, UnifiedJobKind::Crawl);
    assert!(lifecycle_status_active(summary.status));

    // Simulate a worker claiming and completing the job: transition it to
    // Running then Completed, and confirm it leaves `pending`/`queued`.
    store
        .update_status(axon_api::source::JobStatusUpdate {
            job_id: JobId(job_id),
            source_id: None,
            stage_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Fetching,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .unwrap();
    let running = store.get(JobId(job_id)).await.unwrap().unwrap();
    assert_eq!(running.status, LifecycleStatus::Running);

    store
        .update_status(axon_api::source::JobStatusUpdate {
            job_id: JobId(job_id),
            source_id: None,
            stage_id: None,
            status: LifecycleStatus::Completed,
            phase: PipelinePhase::Fetching,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .unwrap();

    // Once terminal, the in-flight guard must report it inactive again.
    assert!(!crawl_job_active(&pool, job_id).await);
}

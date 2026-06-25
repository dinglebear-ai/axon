use super::*;
use crate::backend::{JobBackend, JobKind, JobPayload};

#[tokio::test]
async fn sqlite_backend_enqueue_and_list() {
    let path = format!("/tmp/axon-test-{}.db", uuid::Uuid::new_v4());
    let backend = SqliteJobBackend::new_with_path(&path)
        .await
        .expect("SqliteJobBackend::new_with_path should succeed");

    let id = backend
        .enqueue(JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        })
        .await
        .expect("enqueue");

    let jobs = backend.list_jobs(JobKind::Crawl).await.expect("list");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, id);

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
async fn sqlite_backend_cancel_job() {
    let path = format!("/tmp/axon-test-{}.db", uuid::Uuid::new_v4());
    let backend = SqliteJobBackend::new_with_path(&path).await.unwrap();

    let id = backend
        .enqueue(JobPayload::Embed {
            input: "test".into(),
            config_json: "{}".into(),
        })
        .await
        .unwrap();

    let canceled = backend.cancel_job(id, JobKind::Embed).await.unwrap();
    assert!(canceled);

    let status = backend
        .job_status(id, JobKind::Embed)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.status, crate::status::JobStatus::Canceled);

    std::fs::remove_file(&path).ok();
}

#[tokio::test]
#[ignore] // Only runs with 'cargo test -- --ignored' (needs live TEI/Qdrant — not required here)
async fn sqlite_backend_full_job_lifecycle() {
    let path = format!("/tmp/axon-e2e-{}.db", uuid::Uuid::new_v4());
    let backend = SqliteJobBackend::new_with_path(&path).await.unwrap();

    let id = backend
        .enqueue(JobPayload::Embed {
            input: "hello world test content for SQLite runtime".into(),
            config_json: "{}".into(),
        })
        .await
        .unwrap();

    // Job should be pending immediately after enqueue
    let status = backend
        .job_status(id, JobKind::Embed)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.status, crate::status::JobStatus::Pending);

    backend.clear_jobs(JobKind::Embed).await.unwrap();
    std::fs::remove_file(&path).ok();
}

use super::*;
use crate::core::config::Config;
use crate::jobs::backend::JobPayload;
use crate::jobs::ops::enqueue_job;
use crate::jobs::store::open_sqlite_pool;
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
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://e/a/".into(),
            config_json: "{}".into(),
        },
        &cfg,
    )
    .await
    .unwrap();
    assert!(crawl_job_active(&pool, id).await);
    assert!(!crawl_job_active(&pool, Uuid::new_v4()).await);
}

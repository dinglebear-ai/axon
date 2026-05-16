use super::*;
use crate::core::config::Config;
use crate::jobs::backend::JobPayload;
use crate::jobs::lite::ops::enqueue_job;
use crate::jobs::lite::store::open_sqlite_pool;
use std::sync::Arc;

#[tokio::test]
async fn cancel_token_fires_immediately_for_same_process() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".into(),
            config_json: "{}".into(),
        },
        &Config::default_lite(),
    )
    .await
    .unwrap();

    let store = Arc::new(CancelStore::new());
    let token = store.register(id, "attempt-1");

    store.cancel(id, &pool, JobKind::Crawl).await.unwrap();

    assert!(token.is_cancelled(), "token should be cancelled");
}

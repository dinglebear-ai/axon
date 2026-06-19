use super::*;
use crate::core::config::Config;
use crate::crawl::engine::{AdaptiveCrawlSnapshot, CrawlSummary};
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::ops::{claim_next_pending_for_attempt, enqueue_job};
use crate::jobs::store::open_sqlite_pool;

#[tokio::test]
async fn crawl_progress_persister_includes_adaptive_concurrency_snapshot() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let cfg = Config::default_minimal();
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com".to_string(),
            config_json: "{}".to_string(),
        },
        &cfg,
    )
    .await
    .expect("enqueue");
    let attempt = claim_next_pending_for_attempt(&pool, JobKind::Crawl)
        .await
        .expect("claim")
        .expect("claimed");

    let (tx, task) = spawn_crawl_progress_persister(
        &pool,
        id,
        Some(attempt.attempt_id),
        std::path::PathBuf::from("/tmp/axon-crawl-progress"),
    );
    tx.send(CrawlSummary {
        pages_seen: 2,
        pages_discovered: 4,
        adaptive: Some(AdaptiveCrawlSnapshot {
            successes: 10,
            failures: 1,
            lag_events: 0,
            syncs: 2,
            current_target: 5,
            available_permits: 4,
        }),
        ..CrawlSummary::default()
    })
    .await
    .expect("send progress");
    drop(tx);
    task.await.expect("progress task");

    let progress_json: Option<String> =
        sqlx::query_scalar("SELECT progress_json FROM axon_crawl_jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&pool)
            .await
            .expect("progress json");
    let value: serde_json::Value =
        serde_json::from_str(progress_json.as_deref().expect("stored progress json"))
            .expect("valid json");

    assert_eq!(value["phase"], "crawling");
    assert_eq!(value["lifecycle_progress"], serde_json::json!(0.5));
    assert_eq!(value["pages_crawled"], 2);
    assert_eq!(value["adaptive_concurrency"]["current_target"], 5);
    assert_eq!(value["adaptive_concurrency"]["available_permits"], 4);
    assert_eq!(value["adaptive_concurrency"]["successes"], 10);
    assert_eq!(value["adaptive_concurrency"]["failures"], 1);
}

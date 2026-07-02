use super::*;
use crate::backend::{JobKind, JobPayload};
use crate::ops::{claim_next_pending_for_attempt, enqueue_job};
use crate::store::open_sqlite_pool;
use axon_core::config::Config;
use axon_crawl::engine::{AdaptiveCrawlSnapshot, CrawlSummary};

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

#[test]
fn active_ratio_preserves_explicit_zero_progress() {
    assert_eq!(active_ratio(0.0, 100.0), 0.0);
    assert_eq!(active_ratio(1.0, 100.0), 0.02);
}

#[test]
fn legacy_progress_event_preserves_progress_shape() {
    let id = uuid::Uuid::new_v4();
    let progress = serde_json::json!({
        "phase": "embedding",
        "status": "running",
        "attempt": 2,
        "docs_total": 4,
        "docs_embedded": 3,
        "chunks_total": 12,
        "chunks_embedded": 9,
        "bytes_total": 1200,
        "bytes_done": 900,
        "current_path": "src/lib.rs",
        "canonical_uri": "file:///repo",
        "message": "embedding src/lib.rs",
        "warning": "provider is slow"
    });

    let event = legacy_progress_event(id, JobKind::Embed, &progress, 42);

    assert_eq!(event.job_id, JobId::new(id));
    assert_eq!(event.sequence, 42);
    assert_eq!(event.attempt, 2);
    assert_eq!(event.phase, PipelinePhase::Embedding);
    assert_eq!(event.status, LifecycleStatus::Running);
    assert_eq!(event.counts.documents_total, Some(4));
    assert_eq!(event.counts.documents_done, 3);
    assert_eq!(event.counts.chunks_total, Some(12));
    assert_eq!(event.counts.chunks_done, 9);
    assert_eq!(
        event
            .current
            .as_ref()
            .and_then(|current| current.source_item_key.as_ref()),
        Some(&SourceItemKey::new("src/lib.rs"))
    );
    assert_eq!(
        event.warning.as_ref().map(|warning| warning.code.as_str()),
        Some("legacy_worker.warning")
    );
    assert!(event.error.is_none());
}

#[test]
fn legacy_progress_event_carries_structured_error() {
    let event = legacy_progress_event(
        uuid::Uuid::new_v4(),
        JobKind::Crawl,
        &serde_json::json!({"phase": "crawling", "error": "fetch failed"}),
        1,
    );

    assert_eq!(event.severity, Severity::Failed);
    assert_eq!(
        event.error.as_ref().map(|error| error.code.to_string()),
        Some("legacy_worker.error".to_string())
    );
}

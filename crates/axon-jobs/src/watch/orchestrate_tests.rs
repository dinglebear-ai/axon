use super::*;
use crate::store::open_sqlite_pool;
use crate::watch::url_state::{UrlState, upsert_url_state};
use crate::watch::{WatchDefCreate, create_watch_def_with_pool};
use axon_core::config::Config;
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::NamedTempFile;

fn test_cfg(path: &std::path::Path) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = path.to_path_buf();
    cfg
}

/// Create a real watch def row so url_state's FK (watch_id → axon_watch_defs)
/// is satisfied, returning its id.
async fn make_watch(pool: &SqlitePool) -> Uuid {
    let def = create_watch_def_with_pool(
        pool,
        &WatchDefCreate {
            name: "t".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({ "urls": ["https://example.com/docs/page"] }),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await
    .unwrap();
    def.id
}

/// Core single-flight guarantee, offline: when a changed cluster's member already
/// references an active (pending) source crawl, the cluster is reported skipped
/// and NO new crawl is enqueued.
#[tokio::test]
async fn in_flight_cluster_is_skipped_no_new_crawl() {
    let temp = NamedTempFile::new().unwrap();
    let cfg = test_cfg(temp.path());
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();

    let watch_id = make_watch(&pool).await;
    let url = "https://example.com/docs/page";

    // Enqueue a source crawl job via the unified store; queued/pending counts
    // as active for the in-flight guard.
    let active_job = enqueue_change_crawl(&pool, &cfg, "https://example.com/docs/", 2)
        .await
        .unwrap();

    // Seed the URL's snapshot row pointing at that active crawl.
    let state = UrlState {
        last_crawl_job_id: Some(active_job),
        ..Default::default()
    };
    upsert_url_state(&pool, watch_id, url, &state)
        .await
        .unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'source'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let changed = vec![url.to_string()];
    let (clusters, dispatched, errors) =
        dispatch_clusters(&pool, &cfg, watch_id, &changed, 2).await;

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'source'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        after, before,
        "in-flight cluster must not enqueue a new crawl"
    );
    assert!(dispatched.is_empty(), "nothing dispatched: {dispatched:?}");
    assert!(errors.is_empty(), "no errors expected: {errors:?}");
    assert_eq!(clusters.len(), 1);
    assert_eq!(
        clusters[0].get("skipped").and_then(|v| v.as_str()),
        Some("crawl in flight"),
        "cluster must be reported skipped: {:?}",
        clusters[0]
    );
}

/// Counterpart: with no in-flight crawl referenced, the changed cluster enqueues
/// exactly one new source crawl and reports its job id.
#[tokio::test]
async fn idle_cluster_enqueues_one_crawl() {
    let temp = NamedTempFile::new().unwrap();
    let cfg = test_cfg(temp.path());
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();

    let watch_id = make_watch(&pool).await;
    let url = "https://example.com/docs/page";
    // Seed a snapshot row with no crawl id — mirrors detect_url_change, which
    // always upserts the fresh snapshot before dispatch_clusters runs. Without
    // a last_crawl_job_id the URL is not in flight.
    upsert_url_state(&pool, watch_id, url, &UrlState::default())
        .await
        .unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'source'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let changed = vec![url.to_string()];
    let (clusters, dispatched, errors) =
        dispatch_clusters(&pool, &cfg, watch_id, &changed, 2).await;

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'source'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(after, before + 1, "idle cluster enqueues exactly one crawl");
    assert_eq!(dispatched.len(), 1, "one crawl dispatched");
    assert!(errors.is_empty(), "no errors expected: {errors:?}");
    assert_eq!(clusters.len(), 1);
    assert!(
        clusters[0].get("crawl_job_id").is_some(),
        "cluster carries the new crawl id: {:?}",
        clusters[0]
    );

    // The member's snapshot row was updated with the new crawl id via the
    // targeted set_crawl_job_id UPDATE.
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT last_crawl_job_id FROM axon_watch_url_state WHERE watch_id = ? AND url = ?",
    )
    .bind(watch_id.to_string())
    .bind(url)
    .fetch_optional(&pool)
    .await
    .unwrap()
    .flatten();
    assert_eq!(stored.as_deref(), Some(dispatched[0].as_str()));
}

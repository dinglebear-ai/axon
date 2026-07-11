use super::*;
use axon_core::config::Config;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[allow(dead_code)]
fn _assert_signatures() {
    async fn _f1(cfg: &Config) {
        let _: Result<Vec<WatchDef>, _> = list_watch_defs(cfg, 10_i64).await;
    }
    async fn _f2(cfg: &Config, input: &WatchDefCreate) {
        let _: Result<WatchDef, _> = create_watch_def(cfg, input).await;
    }
    async fn _f3(cfg: &Config, id: Uuid) {
        let _: Result<Vec<WatchRun>, _> = list_watch_runs(cfg, id, 10_i64).await;
    }
}

async fn open_pool() -> (SqlitePool, NamedTempFile) {
    let temp = NamedTempFile::new().expect("tempfile");
    let pool = axon_jobs::store::open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .expect("open pool");
    (pool, temp)
}

fn watch_request(source: &str, every_seconds: u64) -> WatchRequest {
    WatchRequest {
        source: source.to_string(),
        schedule: WatchSchedule {
            every_seconds,
            cron: None,
            timezone: None,
        },
        embed: false,
        options: AdapterOptions::default(),
        scope: None,
        collection: None,
        enabled: Some(true),
    }
}

/// `create_source_watch` writes the canonical `SqliteWatchStore` row (the
/// store `list`/`get`/`update`/`pause`/`resume`/`delete` all act on) AND
/// dual-writes a legacy `axon_watch_defs` row so the still-live scheduler
/// (`crates/axon-jobs/src/workers/watch_scheduler.rs`) ticks the watch.
#[tokio::test]
async fn create_source_watch_writes_canonical_and_legacy_rows() {
    let (pool, _temp) = open_pool().await;
    let cfg = Config::test_default();

    let created = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/docs", 60),
    )
    .await
    .expect("create_source_watch");
    assert_eq!(created.canonical_uri, "https://example.com/docs");
    assert_eq!(created.schedule.every_seconds, 60);

    // Canonical store: findable via the same trait `get`/`list`/`update`/etc.
    // resolve through.
    let fetched = SourceWatchStoreTrait::get(
        &open_source_watch_store(&cfg, Some(&pool)).await.unwrap(),
        created.watch_id.clone(),
    )
    .await
    .unwrap();
    assert!(fetched.is_some(), "canonical watch row must be findable");

    // Legacy dual-write: a `watch` task_type row referencing the same URL so
    // the scheduler ticks it.
    let legacy = list_watch_defs_with_pool(&pool, 50).await.unwrap();
    assert!(
        legacy.iter().any(|def| {
            def.task_type == "watch"
                && def
                    .task_payload
                    .get("urls")
                    .and_then(|urls| urls.as_array())
                    .is_some_and(|urls| {
                        urls.iter()
                            .any(|u| u.as_str() == Some("https://example.com/docs"))
                    })
        }),
        "expected a dual-written legacy watch_def for the created source watch"
    );
}

/// `every_seconds` outside the legacy watch's bounds (`MIN`/`MAX_WATCH_INTERVAL_SECS`,
/// see `axon-jobs::watch::validation`) must not fail the canonical create — the
/// dual-write is best-effort and only logs a warning.
#[tokio::test]
async fn create_source_watch_survives_legacy_dual_write_validation_failure() {
    let (pool, _temp) = open_pool().await;
    let cfg = Config::test_default();

    // 1 second is below MIN_WATCH_INTERVAL_SECS (30), so the legacy dual-write
    // must fail validation while the canonical create still succeeds.
    let created = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/repo2", 1),
    )
    .await
    .expect("create_source_watch must succeed even if the dual-write is rejected");
    assert_eq!(created.canonical_uri, "https://example.com/repo2");

    let legacy = list_watch_defs_with_pool(&pool, 50).await.unwrap();
    assert!(
        legacy.iter().all(|def| {
            !def.task_payload
                .get("urls")
                .and_then(|urls| urls.as_array())
                .is_some_and(|urls| {
                    urls.iter()
                        .any(|u| u.as_str() == Some("https://example.com/repo2"))
                })
        }),
        "an invalid every_seconds must not persist a legacy watch_def row"
    );
}

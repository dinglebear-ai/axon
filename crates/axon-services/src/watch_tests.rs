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

/// `create_source_watch` writes only the canonical `SqliteWatchStore` row.
#[tokio::test]
async fn create_source_watch_writes_only_canonical_row() {
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

    let legacy = list_watch_defs_with_pool(&pool, 50).await.unwrap();
    assert!(
        legacy.is_empty(),
        "canonical watch create must not dual-write legacy watch_defs"
    );
}

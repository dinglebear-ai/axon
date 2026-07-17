use super::*;
use axon_core::config::Config;
use std::sync::Arc;
use tempfile::NamedTempFile;

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
    let (pool, temp) = open_pool().await;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().to_path_buf();

    let created = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/docs", 60),
        None,
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

    let legacy_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name IN ('axon_watch_defs', 'axon_watch_runs')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        legacy_tables, 0,
        "canonical watch create must not leave retired watch tables in schema"
    );
}

#[tokio::test]
async fn create_source_watch_ensures_existing_canonical_source() {
    let (pool, _temp) = open_pool().await;
    let cfg = Config::test_default();

    let created = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/docs/", 60),
        None,
    )
    .await
    .expect("create source watch");
    assert_eq!(created.canonical_uri, "https://example.com/docs");

    let ensured = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/docs", 120),
        None,
    )
    .await
    .expect("ensure existing source watch");
    assert_eq!(ensured.watch_id, created.watch_id);
    assert_eq!(ensured.source_id, created.source_id);
    assert_eq!(ensured.canonical_uri, "https://example.com/docs");
    assert_eq!(ensured.schedule.every_seconds, 120);

    let store = open_source_watch_store(&cfg, Some(&pool)).await.unwrap();
    let page = SourceWatchStoreTrait::list(
        &store,
        WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);

    let resolved = resolve_source_watch_id(&cfg, Some(&pool), "https://example.com/docs/")
        .await
        .expect("resolve noisy source through canonical watch");
    assert_eq!(resolved, created.watch_id);
}

#[tokio::test]
async fn source_watch_denies_local_session_scope_without_local_auth() {
    let (pool, temp) = open_pool().await;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().to_path_buf();
    let auth_without_local = AuthSnapshot::default();
    let session_source = "session:claude:/tmp/axon-session-watch-local";

    let err = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request(session_source, 60),
        Some(auth_without_local.clone()),
    )
    .await
    .expect_err("session watch create should require local scope");
    assert!(
        err.to_string().contains("axon:local"),
        "unexpected create error: {err}"
    );

    let created = create_source_watch(&cfg, Some(&pool), watch_request(session_source, 60), None)
        .await
        .expect("trusted local create");
    let ctx = crate::context::ServiceContext::new(Arc::new(cfg))
        .await
        .expect("service context");
    let err = exec_source_watch(
        &ctx,
        Some(&pool),
        created.watch_id,
        WatchExecRequest {
            reason: None,
            refresh: None,
            wait: None,
        },
        Some(auth_without_local),
    )
    .await
    .expect_err("session watch exec should require local scope");
    assert!(
        err.to_string().contains("axon:local"),
        "unexpected exec error: {err}"
    );
}

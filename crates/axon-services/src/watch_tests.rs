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
        limits: Default::default(),
        metadata: Default::default(),
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
async fn overlapping_watch_exec_enqueues_and_links_one_source_job() {
    let (pool, temp) = open_pool().await;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = temp.path().to_path_buf();
    let created = create_source_watch(
        &cfg,
        Some(&pool),
        watch_request("https://example.com/watch-race", 60),
        None,
    )
    .await
    .expect("create watch");
    let ctx = Arc::new(
        crate::context::ServiceContext::new(Arc::new(cfg.clone()))
            .await
            .expect("service context"),
    );
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let execute = |ctx: Arc<crate::context::ServiceContext>, pool: SqlitePool| {
        let watch_id = created.watch_id.clone();
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            exec_source_watch(
                &ctx,
                Some(&pool),
                watch_id,
                WatchExecRequest {
                    reason: Some("concurrent-test".to_string()),
                    refresh: None,
                    wait: Some(false),
                },
                None,
            )
            .await
        }
    };
    let (first, second) = tokio::join!(
        execute(ctx.clone(), pool.clone()),
        execute(ctx.clone(), pool.clone())
    );

    let mut successes = Vec::new();
    let mut errors = Vec::new();
    for result in [first, second] {
        match result {
            Ok(job) => successes.push(job),
            Err(error) => errors.push(error.to_string()),
        }
    }
    assert_eq!(successes.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("watch.execution_busy"), "{}", errors[0]);

    let history = history_source_watch(
        &cfg,
        Some(&pool),
        WatchHistoryRequest {
            watch_id: created.watch_id.clone(),
            status: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .expect("watch history");
    assert_eq!(history.jobs.len(), 1);
    assert_eq!(history.jobs[0].id, successes[0].id);

    sqlx::query("UPDATE jobs SET status = 'completed' WHERE job_id = ?")
        .bind(successes[0].job_id.0.to_string())
        .execute(&pool)
        .await
        .expect("mark first manual run terminal");
    exec_source_watch(
        &ctx,
        Some(&pool),
        created.watch_id,
        WatchExecRequest {
            reason: Some("immediate-terminal-rerun".to_string()),
            refresh: None,
            wait: Some(false),
        },
        None,
    )
    .await
    .expect("terminal manual run must release enqueue lease immediately");
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

#[test]
fn watch_exec_replays_source_request_and_only_applies_execution_overrides() {
    let mut watch = watch_request("skills.sh:search", 300);
    watch.embed = true;
    watch.scope = Some(SourceScope::Api);
    watch.collection = Some("artifact-catalog".to_string());
    watch.limits.max_items = Some(12);
    watch.limits.max_total_bytes = Some(1_048_576);
    watch.metadata.insert(
        "artifact_candidate_mode".to_string(),
        serde_json::json!("refresh"),
    );
    watch
        .options
        .values
        .insert("query".to_string(), serde_json::json!("mcp servers"));
    watch
        .options
        .values
        .insert("owner".to_string(), serde_json::json!("dinglebear-ai"));

    let created = source_request_for_watch_create(&watch);
    let exec = source_request_for_watch_exec(
        watch,
        &WatchExecRequest {
            reason: Some("scheduled refresh".to_string()),
            refresh: Some(SourceRefreshPolicy::Force),
            wait: Some(true),
        },
    );

    assert_eq!(exec.source, created.source);
    assert_eq!(exec.options, created.options);
    assert_eq!(exec.limits, created.limits);
    assert_eq!(exec.scope, created.scope);
    assert_eq!(exec.collection, created.collection);
    assert_eq!(exec.embed, created.embed);
    assert_eq!(exec.intent, SourceIntent::Watch);
    assert_eq!(exec.watch, SourceWatchPolicy::Enabled);
    assert_eq!(exec.refresh, SourceRefreshPolicy::Force);
    assert_eq!(exec.execution.mode, ExecutionMode::Wait);
    assert_eq!(
        exec.metadata.get("artifact_candidate_mode"),
        Some(&serde_json::json!("refresh"))
    );
    assert_eq!(
        exec.metadata.get("watch_exec_reason"),
        Some(&serde_json::json!("scheduled refresh"))
    );
    assert_eq!(created.refresh, SourceRefreshPolicy::IfStale);
}

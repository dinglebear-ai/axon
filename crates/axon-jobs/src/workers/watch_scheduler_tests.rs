use super::*;
use crate::boundary::WatchStore;
use axon_api::source::{
    AdapterOptions, AdapterRef, AuthSnapshot, SourceId, SourceScope, WatchRequest, WatchSchedule,
};
use sqlx::Row;
use tempfile::NamedTempFile;

async fn scheduler_pool() -> (SqlitePool, NamedTempFile) {
    let temp = NamedTempFile::new().expect("tempfile");
    let pool = crate::store::open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .expect("open pool");
    (pool, temp)
}

fn source_watch_request() -> WatchRequest {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "artifact_candidate_mode".to_string(),
        serde_json::json!("refresh"),
    );
    WatchRequest {
        source: "https://example.com/docs".to_string(),
        schedule: WatchSchedule {
            every_seconds: 60,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: AdapterOptions::default(),
        limits: axon_api::source::SourceLimits {
            max_items: Some(9),
            ..Default::default()
        },
        metadata,
        scope: Some(SourceScope::Docs),
        collection: Some("source-watch-scheduler-test".to_string()),
        enabled: Some(true),
    }
}

async fn create_source_watch_with_auth(
    store: &SqliteWatchStore,
    request: WatchRequest,
) -> axon_api::source::WatchResult {
    store
        .create_with_auth(request, Some(AuthSnapshot::panel("watch-test")))
        .await
        .expect("create source watch")
}

async fn make_source_watch_due(pool: &SqlitePool, watch_id: &str) {
    sqlx::query(
        "UPDATE axon_source_watches SET next_run_at = ?, lease_expires_at = NULL WHERE watch_id = ?",
    )
    .bind(now_ms() - 1_000)
    .bind(watch_id)
    .execute(pool)
    .await
    .expect("mark source watch due");
}

async fn count_rows(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar::<_, i64>(&sql)
        .fetch_one(pool)
        .await
        .expect("count rows")
}

async fn table_exists(pool: &SqlitePool, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("table exists query")
        > 0
}

#[test]
fn parse_tick_secs_defaults_when_absent_or_invalid() {
    assert_eq!(parse_tick_secs(None), DEFAULT_TICK_SECS);
    assert_eq!(
        parse_tick_secs(Some("not-a-number".to_string())),
        DEFAULT_TICK_SECS
    );
    // Zero is rejected — a 0s ticker would busy-spin.
    assert_eq!(parse_tick_secs(Some("0".to_string())), DEFAULT_TICK_SECS);
}

#[test]
fn parse_tick_secs_accepts_valid_override() {
    assert_eq!(parse_tick_secs(Some("5".to_string())), 5);
}

#[test]
fn parse_lease_secs_defaults_when_absent_or_invalid() {
    assert_eq!(parse_lease_secs(None), DEFAULT_LEASE_SECS);
    assert_eq!(parse_lease_secs(Some("0".to_string())), DEFAULT_LEASE_SECS);
    assert_eq!(
        parse_lease_secs(Some("-10".to_string())),
        DEFAULT_LEASE_SECS
    );
}

#[test]
fn parse_lease_secs_accepts_valid_override() {
    assert_eq!(parse_lease_secs(Some("120".to_string())), 120);
}

#[tokio::test]
async fn sweep_enqueues_due_source_watch_without_legacy_rows() {
    let (pool, _temp) = scheduler_pool().await;
    let source_store = SqliteWatchStore::new(pool.clone());
    let created = create_source_watch_with_auth(&source_store, source_watch_request()).await;
    make_source_watch_due(&pool, &created.watch_id.0).await;

    assert!(!table_exists(&pool, "axon_watch_defs").await);
    assert!(!table_exists(&pool, "axon_watch_runs").await);

    let before = now_ms();
    let fired = sweep_due_watches(
        &Arc::new(pool.clone()),
        &Arc::new(Config::default_minimal()),
        &Arc::new(Notify::new()),
        60_000,
    )
    .await
    .expect("sweep");

    assert_eq!(fired, 1);
    assert!(!table_exists(&pool, "axon_watch_defs").await);
    assert!(!table_exists(&pool, "axon_watch_runs").await);

    let row = sqlx::query(
        "SELECT job_id, kind, intent, status, source_id, watch_id, request_json, metadata_json, idempotency_key, auth_snapshot_json \
         FROM jobs",
    )
    .fetch_one(&pool)
    .await
    .expect("queued source job");
    let job_id: String = row.get("job_id");
    assert_eq!(row.get::<String, _>("kind"), "source");
    assert_eq!(row.get::<String, _>("intent"), "watch");
    assert_eq!(row.get::<String, _>("status"), "queued");
    assert_eq!(row.get::<Option<String>, _>("source_id"), None);
    assert_eq!(row.get::<Option<String>, _>("watch_id"), None);
    assert!(
        row.get::<String, _>("idempotency_key")
            .starts_with(&format!("source-watch:{}:", created.watch_id.0))
    );

    let request_json: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("request_json")).expect("request json");
    assert_eq!(
        request_json["source_request"]["source"],
        "https://example.com/docs"
    );
    assert_eq!(request_json["source_request"]["intent"], "watch");
    assert_eq!(request_json["source_request"]["watch"], "enabled");
    assert_eq!(request_json["source_request"]["limits"]["max_items"], 9);
    assert_eq!(
        request_json["source_request"]["metadata"]["artifact_candidate_mode"],
        "refresh"
    );
    assert_eq!(
        request_json["source_request"]["metadata"]["source_watch_id"],
        created.watch_id.0
    );

    let metadata_json: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("metadata_json")).expect("metadata json");
    assert_eq!(metadata_json["source_watch_id"], created.watch_id.0);
    let child_auth_json: String = row.get("auth_snapshot_json");
    let watch_auth_json: String =
        sqlx::query_scalar("SELECT auth_snapshot_json FROM axon_source_watches WHERE watch_id = ?")
            .bind(&created.watch_id.0)
            .fetch_one(&pool)
            .await
            .expect("watch auth snapshot");
    assert_eq!(
        child_auth_json, watch_auth_json,
        "scheduled child job must clone the watch caller snapshot byte-for-byte"
    );

    let run = sqlx::query("SELECT watch_id, job_id FROM axon_source_watch_runs")
        .fetch_one(&pool)
        .await
        .expect("source watch run");
    assert_eq!(run.get::<String, _>("watch_id"), created.watch_id.0);
    assert_eq!(run.get::<String, _>("job_id"), job_id);

    let watch = sqlx::query(
        "SELECT last_job_id, last_status, lease_expires_at, next_run_at FROM axon_source_watches \
         WHERE watch_id = ?",
    )
    .bind(&created.watch_id.0)
    .fetch_one(&pool)
    .await
    .expect("source watch row");
    assert_eq!(watch.get::<Option<String>, _>("last_job_id"), Some(job_id));
    assert_eq!(
        watch.get::<Option<String>, _>("last_status"),
        Some("queued".to_string())
    );
    assert_eq!(watch.get::<Option<i64>, _>("lease_expires_at"), None);
    assert!(watch.get::<i64, _>("next_run_at") >= before + 60_000);
}

#[tokio::test]
async fn sweep_does_not_enqueue_duplicate_while_source_job_is_live() {
    let (pool, _temp) = scheduler_pool().await;
    let source_store = SqliteWatchStore::new(pool.clone());
    let created = create_source_watch_with_auth(&source_store, source_watch_request()).await;
    make_source_watch_due(&pool, &created.watch_id.0).await;

    let pool_arc = Arc::new(pool.clone());
    let cfg = Arc::new(Config::default_minimal());
    let notify = Arc::new(Notify::new());
    assert_eq!(
        sweep_due_watches(&pool_arc, &cfg, &notify, 60_000)
            .await
            .expect("first sweep"),
        1
    );

    make_source_watch_due(&pool, &created.watch_id.0).await;
    assert_eq!(
        sweep_due_watches(&pool_arc, &cfg, &notify, 60_000)
            .await
            .expect("second sweep"),
        0
    );
    assert_eq!(count_rows(&pool, "jobs").await, 1);
    assert_eq!(count_rows(&pool, "axon_source_watch_runs").await, 1);
}

#[tokio::test]
async fn sweep_coalesces_live_refreshes_across_watches_for_same_source() {
    let (pool, _temp) = scheduler_pool().await;
    let source_store = SqliteWatchStore::new(pool.clone());
    let source_id = SourceId::new("src_shared");
    let adapter = AdapterRef {
        name: "web".to_string(),
        version: "test".to_string(),
    };
    let first = source_store
        .create_resolved_with_auth(
            source_watch_request(),
            source_id.clone(),
            "https://example.com/docs".to_string(),
            adapter.clone(),
            Some(AuthSnapshot::panel("watch-test")),
        )
        .await
        .unwrap();
    let second = source_store
        .create_resolved_with_auth(
            source_watch_request(),
            source_id,
            "https://example.com/docs".to_string(),
            adapter,
            Some(AuthSnapshot::panel("watch-test")),
        )
        .await
        .unwrap();
    make_source_watch_due(&pool, &first.watch_id.0).await;
    make_source_watch_due(&pool, &second.watch_id.0).await;

    let fired = sweep_due_watches(
        &Arc::new(pool.clone()),
        &Arc::new(Config::default_minimal()),
        &Arc::new(Notify::new()),
        60_000,
    )
    .await
    .unwrap();

    assert_eq!(fired, 1);
    assert_eq!(count_rows(&pool, "jobs").await, 1);
    assert_eq!(count_rows(&pool, "axon_source_watch_runs").await, 1);
}

#[tokio::test]
async fn sweep_refuses_watch_without_persisted_auth_snapshot() {
    let (pool, _temp) = scheduler_pool().await;
    let source_store = SqliteWatchStore::new(pool.clone());
    let created = WatchStore::create(&source_store, source_watch_request())
        .await
        .expect("create legacy snapshot-less watch");
    make_source_watch_due(&pool, &created.watch_id.0).await;

    let fired = sweep_due_watches(
        &Arc::new(pool.clone()),
        &Arc::new(Config::default_minimal()),
        &Arc::new(Notify::new()),
        60_000,
    )
    .await
    .expect("sweep");

    assert_eq!(fired, 0, "missing caller authority must fail closed");
    assert_eq!(count_rows(&pool, "jobs").await, 0);
    let lease: Option<i64> =
        sqlx::query_scalar("SELECT lease_expires_at FROM axon_source_watches WHERE watch_id = ?")
            .bind(&created.watch_id.0)
            .fetch_one(&pool)
            .await
            .expect("watch lease state");
    assert_eq!(lease, None, "failed dispatch must release the watch lease");
}

#[tokio::test]
async fn sweep_has_no_retired_watch_defs_to_scan() {
    let (pool, _temp) = scheduler_pool().await;
    assert!(!table_exists(&pool, "axon_watch_defs").await);
    assert!(!table_exists(&pool, "axon_watch_runs").await);

    let fired = sweep_due_watches(
        &Arc::new(pool.clone()),
        &Arc::new(Config::default_minimal()),
        &Arc::new(Notify::new()),
        60_000,
    )
    .await
    .expect("sweep");

    assert_eq!(fired, 0, "no canonical source watches were due");
    assert_eq!(count_rows(&pool, "jobs").await, 0);
}

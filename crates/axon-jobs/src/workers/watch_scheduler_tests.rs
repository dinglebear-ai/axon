use super::*;
use axon_api::source::{AdapterOptions, SourceScope, WatchRequest, WatchSchedule};
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
    WatchRequest {
        source: "https://example.com/docs".to_string(),
        schedule: WatchSchedule {
            every_seconds: 60,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: AdapterOptions::default(),
        scope: Some(SourceScope::Docs),
        collection: Some("source-watch-scheduler-test".to_string()),
        enabled: Some(true),
    }
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
    let created = WatchStore::create(&source_store, source_watch_request())
        .await
        .expect("create source watch");
    make_source_watch_due(&pool, &created.watch_id.0).await;

    assert_eq!(count_rows(&pool, "axon_watch_defs").await, 0);
    assert_eq!(count_rows(&pool, "axon_watch_runs").await, 0);

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
    assert_eq!(count_rows(&pool, "axon_watch_defs").await, 0);
    assert_eq!(count_rows(&pool, "axon_watch_runs").await, 0);

    let row = sqlx::query(
        "SELECT job_id, kind, intent, status, source_id, watch_id, request_json, metadata_json, idempotency_key \
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
    assert_eq!(
        request_json["source_request"]["metadata"]["source_watch_id"],
        created.watch_id.0
    );

    let metadata_json: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("metadata_json")).expect("metadata json");
    assert_eq!(metadata_json["source_watch_id"], created.watch_id.0);

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
    let created = WatchStore::create(&source_store, source_watch_request())
        .await
        .expect("create source watch");
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

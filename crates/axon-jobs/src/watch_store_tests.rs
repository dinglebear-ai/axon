use super::*;
use tempfile::NamedTempFile;
use uuid::Uuid;

async fn store() -> (SqliteWatchStore, SqlitePool, NamedTempFile) {
    let temp = NamedTempFile::new().expect("tempfile");
    let pool = crate::store::open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .expect("open pool");
    (SqliteWatchStore::new(pool.clone()), pool, temp)
}

fn watch_request() -> WatchRequest {
    WatchRequest {
        source: "file:///repo".to_string(),
        schedule: WatchSchedule {
            every_seconds: 60,
            cron: None,
            timezone: None,
        },
        embed: true,
        options: AdapterOptions::default(),
        scope: Some(SourceScope::Directory),
        collection: Some("watch-test".to_string()),
        enabled: Some(true),
    }
}

async fn insert_job(pool: &SqlitePool) -> JobId {
    let job_id = JobId::new(Uuid::new_v4());
    let now = Timestamp::from(chrono::Utc::now()).0;
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, created_at, updated_at) \
         VALUES (?, 'source', 'queued', 'queued', 'normal', ?, ?)",
    )
    .bind(job_id.0.to_string())
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .expect("insert job");
    job_id
}

#[tokio::test]
async fn sqlite_watch_store_creates_gets_updates_and_lists() {
    let (store, _pool, _temp) = store().await;

    let created = WatchStore::create(&store, watch_request()).await.unwrap();
    assert!(created.enabled);
    assert_eq!(created.schedule.every_seconds, 60);

    let fetched = WatchStore::get(&store, created.watch_id.clone())
        .await
        .unwrap()
        .expect("watch present");
    assert_eq!(fetched.watch_id, created.watch_id);
    assert_eq!(fetched.canonical_uri, "file:///repo");

    let updated = WatchStore::update(
        &store,
        created.watch_id.clone(),
        WatchUpdateRequest {
            enabled: Some(false),
            schedule: None,
            options: None,
            embed: None,
            collection: None,
            scope: Some(SourceScope::Repo),
        },
    )
    .await
    .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.scope, SourceScope::Repo);

    let listed = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: Some(false),
            source_id: Some(updated.source_id.clone()),
            adapter: Some("sqlite-watch-store".to_string()),
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].watch_id, created.watch_id);
}

#[tokio::test]
async fn sqlite_watch_store_reconstructs_stored_request() {
    let (store, _pool, _temp) = store().await;

    let created = WatchStore::create(&store, watch_request()).await.unwrap();
    let request = store
        .request(created.watch_id.clone())
        .await
        .unwrap()
        .expect("stored request");

    assert_eq!(request.source, "file:///repo");
    assert_eq!(request.schedule.every_seconds, 60);
    assert!(request.embed);
    assert_eq!(request.scope, Some(SourceScope::Directory));
    assert_eq!(request.collection.as_deref(), Some("watch-test"));
    assert_eq!(
        store.request(WatchId::new("missing")).await.unwrap(),
        None,
        "missing canonical ids should not resolve through a fallback"
    );
}

#[tokio::test]
async fn sqlite_watch_store_get_returns_none_for_missing_watch() {
    let (store, _pool, _temp) = store().await;
    let missing = WatchStore::get(&store, WatchId::new("nope")).await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn sqlite_watch_store_update_rejects_missing_watch() {
    let (store, _pool, _temp) = store().await;
    let err = WatchStore::update(
        &store,
        WatchId::new("nope"),
        WatchUpdateRequest {
            enabled: Some(false),
            schedule: None,
            options: None,
            embed: None,
            collection: None,
            scope: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "watch.not_found");
}

#[tokio::test]
async fn sqlite_watch_store_record_run_and_history_round_trip() {
    let (store, pool, _temp) = store().await;
    let watch = WatchStore::create(&store, watch_request()).await.unwrap();
    let job_id = insert_job(&pool).await;

    WatchStore::record_run(&store, watch.watch_id.clone(), job_id)
        .await
        .unwrap();

    let fetched = WatchStore::get(&store, watch.watch_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        fetched.latest_job.as_ref().map(|job| job.job_id),
        Some(job_id)
    );

    let history = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: watch.watch_id.clone(),
            status: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(history.jobs.len(), 1);
    assert_eq!(history.jobs[0].job_id, job_id);
}

#[tokio::test]
async fn sqlite_watch_store_record_run_rejects_dangling_links() {
    let (store, _pool, _temp) = store().await;
    let watch = WatchStore::create(&store, watch_request()).await.unwrap();

    let err = WatchStore::record_run(&store, watch.watch_id.clone(), JobId::new(Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "job.not_found");

    let err = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: WatchId::new("missing"),
            status: None,
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code.to_string(), "watch.not_found");
}

#[tokio::test]
async fn sqlite_watch_store_delete_removes_row() {
    let (store, _pool, _temp) = store().await;
    let watch = WatchStore::create(&store, watch_request()).await.unwrap();

    assert!(store.delete(watch.watch_id.clone()).await.unwrap());
    assert!(!store.delete(watch.watch_id.clone()).await.unwrap());
    assert!(
        WatchStore::get(&store, watch.watch_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn sqlite_watch_store_reset_clears_all_watches() {
    let (store, _pool, _temp) = store().await;
    WatchStore::create(&store, watch_request()).await.unwrap();
    WatchStore::reset(&store).await.unwrap();
    let listed = WatchStore::list(
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
    assert!(listed.items.is_empty());
}

#[tokio::test]
async fn sqlite_watch_store_reports_capabilities() {
    let (store, _pool, _temp) = store().await;
    let capability = WatchStore::capabilities(&store).await.unwrap();
    assert_eq!(capability.0.owner_crate, "axon-jobs");
    assert_eq!(capability.0.name, "sqlite-watch-store");
}

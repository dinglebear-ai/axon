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
    let (store, pool, _temp) = store().await;

    let created = WatchStore::create(&store, watch_request()).await.unwrap();
    assert!(created.enabled);
    assert_eq!(created.schedule.every_seconds, 60);

    let fetched = WatchStore::get(&store, created.watch_id.clone())
        .await
        .unwrap()
        .expect("watch present");
    assert_eq!(fetched.watch_id, created.watch_id);
    assert_eq!(fetched.canonical_uri, "file:///repo");

    let by_source = store
        .find_by_source("file:///repo")
        .await
        .unwrap()
        .expect("source lookup should find watch");
    assert_eq!(by_source.watch_id, created.watch_id);

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

    let forced_next_run_at = 1_700_000_000_000_i64;
    sqlx::query("UPDATE axon_source_watches SET next_run_at = ? WHERE watch_id = ?")
        .bind(forced_next_run_at)
        .bind(&created.watch_id.0)
        .execute(&pool)
        .await
        .unwrap();

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
    assert_eq!(
        listed.items[0].next_run_at,
        Timestamp::from(
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(forced_next_run_at).unwrap()
        )
    );
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
async fn sqlite_watch_store_create_resolved_preserves_canonical_identity() {
    let (store, _pool, _temp) = store().await;
    let created = store
        .create_resolved_with_auth(
            watch_request(),
            SourceId::new("src_canonical_file_repo"),
            "local://lp_repo".to_string(),
            AdapterRef {
                name: "local".to_string(),
                version: "test".to_string(),
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(created.source_id, SourceId::new("src_canonical_file_repo"));
    assert_eq!(created.canonical_uri, "local://lp_repo");
    assert_eq!(created.adapter.name, "local");
    assert_eq!(
        store
            .find_by_source("local://lp_repo")
            .await
            .unwrap()
            .expect("canonical lookup")
            .watch_id,
        created.watch_id
    );
}

#[tokio::test]
async fn sqlite_watch_store_rejects_zero_interval_on_create() {
    let (store, _pool, _temp) = store().await;
    let mut request = watch_request();
    request.schedule.every_seconds = 0;

    let err = WatchStore::create(&store, request)
        .await
        .expect_err("zero interval should be rejected");

    assert_eq!(err.code.to_string(), "watch.invalid_schedule");
}

#[tokio::test]
async fn sqlite_watch_store_rejects_zero_interval_on_update() {
    let (store, _pool, _temp) = store().await;
    let created = WatchStore::create(&store, watch_request()).await.unwrap();

    let err = WatchStore::update(
        &store,
        created.watch_id,
        WatchUpdateRequest {
            enabled: None,
            schedule: Some(WatchSchedule {
                every_seconds: 0,
                cron: None,
                timezone: None,
            }),
            options: None,
            embed: None,
            collection: None,
            scope: None,
        },
    )
    .await
    .expect_err("zero interval should be rejected");

    assert_eq!(err.code.to_string(), "watch.invalid_schedule");
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

#[tokio::test]
async fn sqlite_watch_list_uses_stable_opaque_cursor_pages() {
    let (store, pool, _temp) = store().await;
    for index in 0..3 {
        let mut request = watch_request();
        request.source = format!("file:///repo/{index}");
        let watch = WatchStore::create(&store, request).await.unwrap();
        sqlx::query("UPDATE axon_source_watches SET created_at = ? WHERE watch_id = ?")
            .bind(100 + index)
            .bind(&watch.watch_id.0)
            .execute(&pool)
            .await
            .unwrap();
    }

    let first = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: Some(2),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.total, Some(3));
    let cursor = first.next_cursor.expect("second page cursor");
    assert!(!cursor.contains("watch_"));

    let second = WatchStore::list(
        &store,
        WatchListRequest {
            enabled: None,
            source_id: None,
            adapter: None,
            limit: Some(2),
            cursor: Some(cursor),
        },
    )
    .await
    .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.total, None);
    assert_eq!(second.next_cursor, None);
}

#[tokio::test]
async fn sqlite_watch_history_filters_before_cursor_pagination() {
    let (store, pool, _temp) = store().await;
    let watch = WatchStore::create(&store, watch_request()).await.unwrap();
    for index in 0..4 {
        let job_id = insert_job(&pool).await;
        let status = if index % 2 == 0 {
            "completed"
        } else {
            "failed"
        };
        sqlx::query("UPDATE jobs SET status = ? WHERE job_id = ?")
            .bind(status)
            .bind(job_id.0.to_string())
            .execute(&pool)
            .await
            .unwrap();
        WatchStore::record_run(&store, watch.watch_id.clone(), job_id)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE axon_source_watch_runs SET created_at = ? WHERE watch_id = ? AND job_id = ?",
        )
        .bind(100 + index)
        .bind(&watch.watch_id.0)
        .bind(job_id.0.to_string())
        .execute(&pool)
        .await
        .unwrap();
    }

    let first = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: watch.watch_id.clone(),
            status: Some(LifecycleStatus::Completed),
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(first.jobs.len(), 1);
    let second = WatchStore::history(
        &store,
        WatchHistoryRequest {
            watch_id: watch.watch_id,
            status: Some(LifecycleStatus::Completed),
            limit: Some(1),
            cursor: first.next_cursor,
        },
    )
    .await
    .unwrap();
    assert_eq!(second.jobs.len(), 1);
    assert_eq!(second.next_cursor, None);
}

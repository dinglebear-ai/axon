use super::*;
use crate::jobs::store::open_sqlite_pool;
use crate::jobs::watch::{WatchDefCreate, create_watch_def_with_pool};
use chrono::Utc;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[tokio::test]
async fn snapshot_round_trips_and_upserts() {
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "w".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls":["https://e/a"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    assert!(
        get_url_state(&pool, watch.id, "https://e/a")
            .await
            .unwrap()
            .is_none()
    );

    let s = UrlState {
        etag: Some("\"x\"".into()),
        last_modified: None,
        content_hash: Some("h1".into()),
        last_markdown: Some("# A".into()),
        last_links_json: Some("[]".into()),
        last_checked_at: Some(1),
        last_changed_at: Some(1),
        last_crawl_job_id: Some(Uuid::new_v4()),
    };
    upsert_url_state(&pool, watch.id, "https://e/a", &s)
        .await
        .unwrap();
    assert_eq!(
        get_url_state(&pool, watch.id, "https://e/a")
            .await
            .unwrap()
            .unwrap(),
        s
    );

    let mut s2 = s.clone();
    s2.content_hash = Some("h2".into());
    upsert_url_state(&pool, watch.id, "https://e/a", &s2)
        .await
        .unwrap();
    assert_eq!(
        get_url_state(&pool, watch.id, "https://e/a")
            .await
            .unwrap()
            .unwrap()
            .content_hash
            .as_deref(),
        Some("h2")
    );
}

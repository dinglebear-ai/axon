use super::*;
use crate::store::open_sqlite_pool;

async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config_snapshots.db");
    let pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("open pool applies migration 0025_config_snapshots");
    (dir, pool)
}

#[tokio::test]
async fn stores_and_reads_back_a_snapshot() {
    let (_dir, pool) = test_pool().await;
    upsert_config_snapshot(&pool, "cfg_abc123", r#"{"collection":"axon"}"#)
        .await
        .expect("upsert should succeed");

    let fetched = get_config_snapshot(&pool, "cfg_abc123")
        .await
        .expect("get should succeed");
    assert_eq!(fetched.as_deref(), Some(r#"{"collection":"axon"}"#));
}

#[tokio::test]
async fn unknown_id_returns_none_not_an_error() {
    let (_dir, pool) = test_pool().await;
    let fetched = get_config_snapshot(&pool, "cfg_never_written")
        .await
        .expect("get of an unknown id is Ok(None), not an error");
    assert!(fetched.is_none());
}

#[tokio::test]
async fn duplicate_upsert_of_the_same_id_is_a_no_op() {
    let (_dir, pool) = test_pool().await;
    upsert_config_snapshot(&pool, "cfg_dup", r#"{"a":1}"#)
        .await
        .expect("first upsert");
    // Same id, different body: INSERT OR IGNORE keeps the first-written
    // content, matching the content-addressed contract documented on the
    // migration (the id is a hash of the content, so a real mismatch would
    // indicate caller error, not something this layer should silently fix).
    upsert_config_snapshot(&pool, "cfg_dup", r#"{"a":2}"#)
        .await
        .expect("second upsert with same id is a no-op, not an error");

    let fetched = get_config_snapshot(&pool, "cfg_dup").await.unwrap();
    assert_eq!(fetched.as_deref(), Some(r#"{"a":1}"#));
}

#[tokio::test]
async fn empty_id_is_rejected() {
    let (_dir, pool) = test_pool().await;
    let err = upsert_config_snapshot(&pool, "", r#"{}"#)
        .await
        .expect_err("blank id must be rejected");
    assert_eq!(err.code.to_string(), "config_snapshot.invalid_id");
}

#[tokio::test]
async fn distinct_ids_store_distinct_content() {
    let (_dir, pool) = test_pool().await;
    upsert_config_snapshot(&pool, "cfg_one", r#"{"n":1}"#)
        .await
        .unwrap();
    upsert_config_snapshot(&pool, "cfg_two", r#"{"n":2}"#)
        .await
        .unwrap();

    assert_eq!(
        get_config_snapshot(&pool, "cfg_one").await.unwrap(),
        Some(r#"{"n":1}"#.to_string())
    );
    assert_eq!(
        get_config_snapshot(&pool, "cfg_two").await.unwrap(),
        Some(r#"{"n":2}"#.to_string())
    );
}

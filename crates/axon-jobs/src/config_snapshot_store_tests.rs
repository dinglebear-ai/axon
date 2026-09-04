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
    let json = r#"{"collection":"axon"}"#;
    let id = config_snapshot_id_from_json(json);
    upsert_config_snapshot(&pool, &id, json)
        .await
        .expect("upsert should succeed");

    let fetched = get_config_snapshot(&pool, &id)
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
async fn duplicate_upsert_rejects_content_mismatch() {
    let (_dir, pool) = test_pool().await;
    let first = r#"{"a":1}"#;
    let id = config_snapshot_id_from_json(first);
    upsert_config_snapshot(&pool, &id, first)
        .await
        .expect("first upsert");
    let err = upsert_config_snapshot(&pool, &id, r#"{"a":2}"#)
        .await
        .expect_err("same id with different content must be rejected");

    assert_eq!(err.code.to_string(), "config_snapshot.digest_mismatch");
    let fetched = get_config_snapshot(&pool, &id).await.unwrap();
    assert_eq!(fetched.as_deref(), Some(first));
}

#[tokio::test]
async fn forged_content_id_is_rejected_before_insert() {
    let (_dir, pool) = test_pool().await;
    let err = upsert_config_snapshot(&pool, "cfg_000000000000", r#"{"a":1}"#)
        .await
        .expect_err("id must match the snapshot digest");

    assert_eq!(err.code.to_string(), "config_snapshot.digest_mismatch");
    assert!(
        get_config_snapshot(&pool, "cfg_000000000000")
            .await
            .unwrap()
            .is_none()
    );
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
    let first = r#"{"n":1}"#;
    let second = r#"{"n":2}"#;
    let first_id = config_snapshot_id_from_json(first);
    let second_id = config_snapshot_id_from_json(second);
    upsert_config_snapshot(&pool, &first_id, first)
        .await
        .unwrap();
    upsert_config_snapshot(&pool, &second_id, second)
        .await
        .unwrap();

    assert_eq!(
        get_config_snapshot(&pool, &first_id).await.unwrap(),
        Some(r#"{"n":1}"#.to_string())
    );
    assert_eq!(
        get_config_snapshot(&pool, &second_id).await.unwrap(),
        Some(r#"{"n":2}"#.to_string())
    );
}

use super::*;

#[tokio::test]
async fn sqlite_cutover_ignores_current_unified_content_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jobs.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("open sqlite");
    sqlx::query("CREATE TABLE sources (source_id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create sources");
    sqlx::query("CREATE TABLE memory_records (memory_id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create memory_records");
    sqlx::query("INSERT INTO sources (source_id) VALUES ('src_current')")
        .execute(&pool)
        .await
        .expect("insert source");
    sqlx::query("INSERT INTO memory_records (memory_id) VALUES ('mem_current')")
        .execute(&pool)
        .await
        .expect("insert memory");
    pool.close().await;

    let rows = count_sqlite_legacy_rows(&path).await.expect("legacy count");
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn sqlite_cutover_counts_retired_source_ledger_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jobs.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("open sqlite");
    sqlx::query("CREATE TABLE axon_source_sources (source_id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("create legacy sources");
    sqlx::query("CREATE TABLE axon_source_manifest_items (source_id TEXT, item_key TEXT)")
        .execute(&pool)
        .await
        .expect("create legacy manifest items");
    sqlx::query("INSERT INTO axon_source_sources (source_id) VALUES ('src_old')")
        .execute(&pool)
        .await
        .expect("insert legacy source");
    sqlx::query(
        "INSERT INTO axon_source_manifest_items (source_id, item_key) VALUES ('src_old', 'index')",
    )
    .execute(&pool)
    .await
    .expect("insert legacy manifest");
    pool.close().await;

    let rows = count_sqlite_legacy_rows(&path).await.expect("legacy count");
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn sqlite_cutover_rejects_old_version_one_receipt_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jobs.db");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("open sqlite");
    sqlx::query(
        "CREATE TABLE axon_applied_migrations (namespace TEXT, version INTEGER, name TEXT)",
    )
    .execute(&pool)
    .await
    .expect("create old receipt ledger");
    sqlx::query("PRAGMA user_version = 1")
        .execute(&pool)
        .await
        .expect("stamp ambiguous old version");
    pool.close().await;

    assert!(
        sqlite_schema_identity_is_incompatible(&path)
            .await
            .expect("probe schema identity")
    );
}

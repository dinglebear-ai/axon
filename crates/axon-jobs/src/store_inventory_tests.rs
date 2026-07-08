use super::*;
use crate::store::open_sqlite_pool;

async fn test_pool() -> SqlitePool {
    open_sqlite_pool(":memory:").await.expect("pool")
}

/// `axon_crawl_jobs`/`axon_embed_jobs`/etc. are created by the current
/// migration set (they still exist for read compatibility) — the "legacy"
/// framing is about non-empty rows, not table presence. Seed a row using the
/// real schema rather than a throwaway table shape.
async fn seed_legacy_row(pool: &SqlitePool, table: &str) {
    sqlx::query(&format!(
        "INSERT INTO {table} (id, created_at, updated_at) VALUES ('legacy-1', 0, 0)"
    ))
    .execute(pool)
    .await
    .expect("seed legacy row");
}

#[tokio::test]
async fn no_blocker_when_legacy_tables_absent() {
    let pool = test_pool().await;
    let blocker = detect_incompatible_legacy_jobs(&pool).await.expect("check");
    assert!(blocker.is_none());
}

#[tokio::test]
async fn blocker_reports_non_empty_legacy_tables() {
    let pool = test_pool().await;
    seed_legacy_row(&pool, "axon_crawl_jobs").await;
    seed_legacy_row(&pool, "axon_embed_jobs").await;

    let blocker = detect_incompatible_legacy_jobs(&pool)
        .await
        .expect("check")
        .expect("blocker present");

    assert_eq!(
        blocker.legacy_tables,
        vec!["axon_crawl_jobs", "axon_embed_jobs"]
    );
    assert_eq!(blocker.tables.len(), 2);
    assert!(blocker.tables.iter().all(|t| t.row_count == 1));
    assert!(blocker.message.contains("axon_crawl_jobs=1 rows"));
}

#[tokio::test]
async fn blocker_ignores_tables_with_zero_rows() {
    let pool = test_pool().await;
    // The migration set creates axon_crawl_jobs (read compatibility) but the
    // fresh pool starts with zero rows in it — no blocker should fire.
    let blocker = detect_incompatible_legacy_jobs(&pool).await.expect("check");
    assert!(blocker.is_none());
}

#[tokio::test]
async fn cutover_receipt_suppresses_blocker() {
    let pool = test_pool().await;
    seed_legacy_row(&pool, "axon_crawl_jobs").await;

    record_cutover_receipt(
        &pool,
        RECEIPT_KIND_PREFLIGHT_CLEAN_CUTOVER,
        "operator reviewed legacy rows and confirmed clean cutover",
    )
    .await
    .expect("record receipt");

    let blocker = detect_incompatible_legacy_jobs(&pool).await.expect("check");
    assert!(blocker.is_none());
}

#[tokio::test]
async fn record_cutover_receipt_persists_kind_and_message() {
    let pool = test_pool().await;
    record_cutover_receipt(
        &pool,
        RECEIPT_KIND_LEGACY_RESET,
        "reset wiped legacy tables",
    )
    .await
    .expect("record receipt");

    let row: (String, String) = sqlx::query_as(
        "SELECT receipt_kind, message FROM axon_job_cutover_receipts ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch receipt");

    assert_eq!(row.0, RECEIPT_KIND_LEGACY_RESET);
    assert_eq!(row.1, "reset wiped legacy tables");
}

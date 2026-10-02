use super::*;
use crate::store::open_sqlite_pool;

#[tokio::test]
async fn missing_receipted_table_is_rejected_before_pending_index_migration() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    sqlx::query("DELETE FROM axon_applied_migrations WHERE namespace='graph' AND version=6")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE graph_nodes")
        .execute(&pool)
        .await
        .unwrap();
    let error = apply_all_migrations(&pool).await.unwrap_err().to_string();
    assert!(error.contains("startup.incompatible_store"), "{error}");
    assert!(error.contains("graph_nodes"), "{error}");
    assert!(error.contains("axon reset"), "{error}");
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace='graph' AND version=6",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 0, "rejection must not advance receipts");
}

#[tokio::test]
async fn required_schema_accepts_prior_prefix_and_current_store() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    sqlx::query("DELETE FROM axon_applied_migrations WHERE namespace='graph' AND version=6")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP INDEX idx_graph_stage_receipts_generation")
        .execute(&pool)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    apply_all_migrations(&pool)
        .await
        .expect("prior prefix upgrades");
    apply_all_migrations(&pool)
        .await
        .expect("current store reopens");
    eprintln!("two schema preflights and upgrade: {:?}", started.elapsed());
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace='graph' AND version=6",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 1);
}

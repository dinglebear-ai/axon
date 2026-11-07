use super::*;

#[tokio::test]
async fn private_stage_has_bounded_cache_and_preserves_durable_journal() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let stage = GraphStage::begin(
        live.pool().clone(),
        SourceId("private-cache-source".into()),
        SourceGenerationId("private-cache-generation".into()),
        JobId(uuid::Uuid::new_v4()),
        1,
    )
    .await
    .unwrap();
    let pool = stage.store.pool();
    assert_eq!(pool.options().get_max_connections(), 1);
    let cache: i64 = sqlx::query_scalar("PRAGMA cache_size")
        .fetch_one(pool)
        .await
        .unwrap();
    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await
        .unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(cache, -65_536);
    assert_eq!(journal, "delete");
    assert_eq!(synchronous, 2, "private stages retain FULL durability");
    stage.mark_disposable().await.unwrap();
}

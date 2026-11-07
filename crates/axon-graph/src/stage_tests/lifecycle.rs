use super::*;

#[tokio::test]
async fn stage_receipt_generation_lookup_uses_durable_index() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let rows=sqlx::query("EXPLAIN QUERY PLAN SELECT summary_json FROM graph_stage_receipts WHERE source_id=? AND generation_id=? ORDER BY activated_at DESC LIMIT 1").bind("s").bind("g").fetch_all(live.pool()).await.unwrap();
    use sqlx::Row;
    assert!(rows.iter().any(|row| {
        row.get::<String, _>("detail")
            .contains("idx_graph_stage_receipts_generation")
    }));
}

#[tokio::test]
async fn dropped_stage_retains_owner_until_private_connections_settle() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let stage = GraphStage::begin_with_private_connections(
        live.pool().clone(),
        SourceId::new("b"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
        2,
    )
    .await
    .unwrap();
    let id = stage.id().to_string();
    let path: String = sqlx::query_scalar("SELECT path FROM graph_stages WHERE stage_id=?")
        .bind(&id)
        .fetch_one(live.pool())
        .await
        .unwrap();
    // Test-only connection checkout deliberately keeps the private pool busy
    // after its handle is dropped, modelling queued/canceled SQLx work.
    let held = stage.test_acquire_private_connection().await;
    drop(stage);
    // Let the settlement thread drain the forced idle connection before
    // checking that the held connection still prevents disposal.
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    GraphStage::mark_disposable_ids(live.pool(), std::slice::from_ref(&id))
        .await
        .unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM graph_stages WHERE stage_id=?")
        .bind(&id)
        .fetch_one(live.pool())
        .await
        .unwrap();
    assert_eq!(state, "building");
    assert!(std::path::Path::new(&path).exists());
    drop(held);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        GraphStage::mark_disposable_ids(live.pool(), std::slice::from_ref(&id))
            .await
            .unwrap();
        if GraphStage::reap(live.pool(), std::slice::from_ref(&id))
            .await
            .unwrap()
            == 1
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("stage owner did not release after its private pool settled");
}

#[tokio::test]
async fn graph_journal_and_external_side_effects_share_one_budget() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let stage = GraphStage::begin(
        live.pool().clone(),
        SourceId::new("b"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
    )
    .await
    .unwrap();
    stage
        .reserve_side_effect_bytes(256 * 1024 * 1024 - 1)
        .await
        .unwrap();
    let error = stage
        .write_candidates(vec![repo_docs_candidate(
            "b",
            "b",
            vec![ev("b", "sitemap", 0.8)],
        )])
        .await
        .unwrap_err();
    assert!(error.message.contains("combined 256 MiB"));
    assert!(stage.reserve_side_effect_bytes(2).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_nodes")
        .fetch_one(live.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
}

#[tokio::test]
async fn explicit_disposal_waits_for_private_connections_to_settle() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let stage = GraphStage::begin_with_private_connections(
        live.pool().clone(),
        SourceId::new("b"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
        2,
    )
    .await
    .unwrap();
    let held = stage.test_acquire_private_connection().await;
    let disposing = stage.mark_disposable();
    tokio::pin!(disposing);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(25), &mut disposing)
            .await
            .is_err()
    );
    assert_eq!(
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap(),
        0
    );
    drop(held);
    tokio::time::timeout(std::time::Duration::from_secs(5), &mut disposing)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap(),
        1
    );
}

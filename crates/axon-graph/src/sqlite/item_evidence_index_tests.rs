use crate::store::GraphStore;
use sqlx::{Row, SqlitePool};

async fn assert_item_lookup_indexed(pool: &SqlitePool) {
    for sql in [
        "EXPLAIN QUERY PLAN SELECT edge_id FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) = ?2",
        "EXPLAIN QUERY PLAN DELETE FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) = ?2",
    ] {
        let plan = sqlx::query(sql)
            .bind("source")
            .bind("item")
            .fetch_all(pool)
            .await
            .unwrap();
        let details: Vec<String> = plan.iter().map(|row| row.get("detail")).collect();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("idx_graph_evidence_item")
                    && detail.contains("<expr>=?")),
            "item retirement must narrow by source AND item: {details:?}"
        );
    }
}

#[tokio::test]
async fn item_retirement_uses_index_for_fresh_and_existing_graph_databases() {
    let fresh = super::store().await;
    assert_item_lookup_indexed(&fresh.pool).await;
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for migration in crate::migration::MIGRATIONS.iter().take(3) {
        sqlx::raw_sql(migration.sql).execute(&pool).await.unwrap();
    }
    let graph = crate::SqliteGraphStore::from_pool(pool.clone());
    let mut contained = super::ev("contained", "sitemap", 0.8);
    contained.metadata.insert(
        "contained_source_item_key".into(),
        serde_json::json!("contained-item"),
    );
    for candidate in [
        super::repo_docs_candidate(
            "first",
            "source-a",
            vec![super::ev("plain", "sitemap", 0.8), contained],
        ),
        super::repo_docs_candidate(
            "second",
            "source-b",
            vec![super::ev("shared", "sitemap", 0.8)],
        ),
    ] {
        graph.upsert_candidates(vec![candidate]).await.unwrap();
    }
    // An existing schema must gain the new index through the composed migration.
    for migration in crate::migration::MIGRATIONS.iter().skip(3) {
        sqlx::raw_sql(migration.sql).execute(&pool).await.unwrap();
    }
    assert_item_lookup_indexed(&pool).await;
    // The index backfills existing provenance without changing retirement safety.
    for item in ["meta", "contained-item"] {
        let retired = graph
            .retire_item_evidence(
                axon_api::source::SourceId::new("source-a"),
                axon_api::source::SourceItemKey::new(item),
            )
            .await
            .unwrap();
        assert_eq!(
            retired.edges_deleted, 0,
            "other source evidence still supports the edge"
        );
    }
    let sources: Vec<String> = sqlx::query_scalar("SELECT source_id FROM graph_evidence")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(sources, vec!["source-b"]);
}

async fn assert_node_retirement_indexed(pool: &SqlitePool) {
    let plan = sqlx::query("EXPLAIN QUERY PLAN DELETE FROM graph_nodes WHERE stable_key IN (SELECT value FROM json_each(?1)) AND json_array_length(source_ids_json) = 1 AND json_extract(source_ids_json, '$[0]') = ?2 AND NOT EXISTS (SELECT 1 FROM graph_edges e WHERE e.from_node_id = graph_nodes.node_id OR e.to_node_id = graph_nodes.node_id)")
        .bind(r#"["item", "other-item"]"#).bind("source").fetch_all(pool).await.unwrap();
    let details: Vec<String> = plan.iter().map(|row| row.get("detail")).collect();
    assert!(
        details
            .iter()
            .any(|s| s.contains("SEARCH graph_nodes") && s.contains("stable_key=?")),
        "node retirement must use an indexed lookup: {details:?}"
    );
}

#[tokio::test]
async fn node_retirement_lookup_indexed_on_fresh_and_upgraded_databases() {
    let fresh = super::store().await;
    assert_node_retirement_indexed(&fresh.pool).await;
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    for migration in crate::migration::MIGRATIONS.iter().take(4) {
        sqlx::raw_sql(migration.sql).execute(&pool).await.unwrap();
    }
    for migration in crate::migration::MIGRATIONS.iter().skip(4) {
        sqlx::raw_sql(migration.sql).execute(&pool).await.unwrap();
    }
    assert_node_retirement_indexed(&pool).await;
}

use super::*;

#[tokio::test]
async fn private_journal_redacts_before_persistence_and_replay() {
    let directory =
        std::env::temp_dir().join(format!("axon-stage-redaction-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let live = SqliteGraphStore::connect(directory.join("live.sqlite").to_str().unwrap())
        .await
        .unwrap();
    let stage = GraphStage::begin(
        live.pool().clone(),
        SourceId::new("b"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
    )
    .await
    .unwrap();
    let secret = "sk-private-stage-journal-secret";
    let mut candidate = repo_docs_candidate("b", "b", vec![ev("b", "github_homepage", 0.9)]);
    candidate.evidence[0].quote = Some(format!("Authorization: Bearer {secret}"));
    candidate.evidence[0]
        .metadata
        .insert("api_key".into(), serde_json::json!(secret));
    candidate.nodes[0]
        .properties
        .insert("token".into(), serde_json::json!(secret));
    candidate.edges[0]
        .properties
        .insert("secret".into(), serde_json::json!(secret));
    stage.write_candidates(vec![candidate]).await.unwrap();
    let path: String = sqlx::query_scalar("SELECT path FROM graph_stages WHERE stage_id=?")
        .bind(stage.id())
        .fetch_one(live.pool())
        .await
        .unwrap();
    let private = sqlx::SqlitePool::connect(&format!("sqlite://{path}"))
        .await
        .unwrap();
    let journal: String = sqlx::query_scalar("SELECT candidate_json FROM stage_journal")
        .fetch_one(&private)
        .await
        .unwrap();
    assert!(!journal.contains(secret));
    let decoded = crate::stage::journal::decode(&journal).unwrap();
    let decoded = serde_json::to_string(&decoded).unwrap();
    assert!(!decoded.contains(secret));
    assert!(decoded.contains("[REDACTED]"));
    let evidence: (Option<String>, String) =
        sqlx::query_as("SELECT quote,metadata_json FROM graph_evidence")
            .fetch_one(&private)
            .await
            .unwrap();
    assert!(!evidence.0.unwrap().contains(secret));
    assert!(!evidence.1.contains(secret));
    // Force current-row replay while preserving the candidate's IDs and authority.
    live.upsert_candidates(vec![repo_docs_candidate(
        "a",
        "a",
        vec![ev("a", "sitemap", 0.4)],
    )])
    .await
    .unwrap();
    let mut conn = live.pool().acquire().await.unwrap();
    stage.prepare_activation(&mut conn).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .unwrap();
    stage.activate_in_tx(&mut conn).await.unwrap();
    sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
    let metadata: String =
        sqlx::query_scalar("SELECT metadata_json FROM graph_evidence WHERE evidence_id='b'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert!(!metadata.contains(secret));
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["redaction_status"], "redacted");
    assert!(
        metadata["redacted_field_count"].as_u64().unwrap()
            + metadata["dropped_field_count"].as_u64().unwrap()
            > 0
    );
    let authority: String = sqlx::query_scalar("SELECT authority FROM graph_edges LIMIT 1")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(authority, "official");
    stage.detach(&mut conn).await.unwrap();
    drop(conn);
    private.close().await;
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
    live.pool().close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn oversized_existing_graph_row_fails_seed_admission_before_json_materialization() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let candidate = repo_docs_candidate(
        "large-seed",
        "source",
        vec![ev("large-seed", "sitemap", 0.8)],
    );
    live.upsert_candidates(vec![candidate.clone()])
        .await
        .unwrap();
    let properties = serde_json::json!({"payload":"x".repeat(8 * 1024 * 1024)}).to_string();
    sqlx::query("UPDATE graph_nodes SET metadata_json=?")
        .bind(properties)
        .execute(live.pool())
        .await
        .unwrap();
    let stage = GraphStage::begin(
        live.pool().clone(),
        SourceId::new("source"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
    )
    .await
    .unwrap();
    let error = stage.write_candidates(vec![candidate]).await.unwrap_err();
    assert!(error.message.contains("seed row exceeds 8 MiB"), "{error}");
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
}

#[tokio::test]
async fn private_graph_rows_reject_growth_beyond_the_stored_window_atomically() {
    let live = SqliteGraphStore::connect(":memory:").await.unwrap();
    let stage = GraphStage::begin_with_private_connections(
        live.pool().clone(),
        SourceId::new("source"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
        2,
    )
    .await
    .unwrap();
    stage
        .write_candidates(vec![repo_docs_candidate(
            "bounded",
            "source",
            vec![ev("bounded", "sitemap", 0.8)],
        )])
        .await
        .unwrap();
    let mut connection = stage.test_acquire_private_connection().await;
    let large = serde_json::json!({"payload":"x".repeat(8 * 1024 * 1024)}).to_string();
    let error = sqlx::query("UPDATE graph_nodes SET metadata_json=?")
        .bind(large)
        .execute(&mut *connection)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stored window"));
    let largest: i64 = sqlx::query_scalar("SELECT max(length(metadata_json)) FROM graph_nodes")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert!(largest < 8 * 1024 * 1024);
    drop(connection);
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
}

#[tokio::test]
async fn compressed_and_legacy_journal_activation_preserve_identical_graph_facts() {
    let mut snapshots = Vec::new();
    for legacy in [true, false] {
        let live = SqliteGraphStore::connect(":memory:").await.unwrap();
        let stage = GraphStage::begin_with_private_connections(
            live.pool().clone(),
            SourceId::new("source"),
            SourceGenerationId::new("g"),
            JobId::new(Uuid::from_u128(7)),
            1,
            2,
        )
        .await
        .unwrap();
        let candidate =
            repo_docs_candidate("identical", "source", vec![ev("identical", "sitemap", 0.8)]);
        stage
            .write_candidates(vec![candidate.clone()])
            .await
            .unwrap();
        if legacy {
            let mut connection = stage.test_acquire_private_connection().await;
            let json = serde_json::to_string(&candidate).unwrap();
            sqlx::query("UPDATE stage_journal SET candidate_json=?")
                .bind(&json)
                .execute(&mut *connection)
                .await
                .unwrap();
            sqlx::query("UPDATE stage_stats SET bytes=?,journal_bytes=?")
                .bind(json.len() as i64)
                .bind(json.len() as i64)
                .execute(&mut *connection)
                .await
                .unwrap();
        }
        let mut connection = live.pool().acquire().await.unwrap();
        stage.prepare_activation(&mut connection).await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .unwrap();
        let result = stage.activate_in_tx(&mut connection).await.unwrap();
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .unwrap();
        stage.detach(&mut connection).await.unwrap();
        drop(connection);
        let nodes: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT node_id,kind,stable_key,metadata_json FROM graph_nodes ORDER BY node_id",
        )
        .fetch_all(live.pool())
        .await
        .unwrap();
        let edges: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT edge_id,kind,from_node_id,to_node_id FROM graph_edges ORDER BY edge_id",
        )
        .fetch_all(live.pool())
        .await
        .unwrap();
        let evidence: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT evidence_id,metadata_json,quote FROM graph_evidence ORDER BY evidence_id",
        )
        .fetch_all(live.pool())
        .await
        .unwrap();
        snapshots.push((
            nodes,
            edges,
            evidence,
            result.candidates_seen,
            result.nodes_upserted,
            result.edges_upserted,
            result.evidence_records,
        ));
        stage.mark_disposable().await.unwrap();
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap();
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

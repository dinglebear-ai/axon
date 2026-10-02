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
    assert!(journal.contains("[REDACTED]"));
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

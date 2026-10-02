use crate::{GraphStage, SqliteGraphStore};
use axon_api::source::{JobId, SourceGenerationId, SourceId};
#[tokio::test]
async fn stage_is_private_and_receipt_rolls_back() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::migration::ensure_schema(&pool).await.unwrap();
    let live = SqliteGraphStore::from_pool(pool);
    let stage = GraphStage::begin(
        live.pool().clone(),
        SourceId::new("s"),
        SourceGenerationId::new("g"),
        JobId::new(Uuid::new_v4()),
        1,
    )
    .await
    .unwrap();
    stage
        .write_candidates(vec![repo_docs_candidate(
            "rollback",
            "s",
            vec![ev("rollback", "sitemap", 0.8)],
        )])
        .await
        .unwrap();
    let mut conn = live.pool().acquire().await.unwrap();
    stage.prepare_activation(&mut conn).await.unwrap();
    assert!(stage.write_candidates(vec![]).await.is_err());
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .unwrap();
    stage.activate_in_tx(&mut conn).await.unwrap();
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM graph_stages WHERE stage_id = ?")
        .bind(stage.id())
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(state, "ready");
    let nodes: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_nodes")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(nodes, 0);
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_stage_receipts")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(receipts, 0);
    stage.detach(&mut conn).await.unwrap();
    drop(conn);
    stage.mark_disposable().await.unwrap();
    assert_eq!(
        GraphStage::reap(live.pool(), &[stage.id().to_string()])
            .await
            .unwrap(),
        1
    );
}

use crate::GraphStore;
use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    MetadataMap, SourceItemKey,
};
use uuid::Uuid;
fn ev(id: &str, kind: &str, confidence: f32) -> GraphEvidence {
    GraphEvidence {
        evidence_id: id.to_string(),
        evidence_kind: kind.to_string(),
        source_id: SourceId::new("src"),
        source_item_key: SourceItemKey::new("item"),
        document_id: None,
        chunk_id: None,
        range: None,
        quote: Some("quote".to_string()),
        confidence,
        metadata: MetadataMap::new(),
    }
}

fn node(kind: &str, key: &str, label: &str) -> GraphNodeCandidate {
    GraphNodeCandidate {
        node_kind: kind.to_string(),
        stable_key: key.to_string(),
        label: label.to_string(),
        properties: MetadataMap::new(),
    }
}

/// A candidate: repo --repo_has_docs--> docs_site, with the given evidence.
fn repo_docs_candidate(id: &str, source: &str, mut evidence: Vec<GraphEvidence>) -> GraphCandidate {
    for item in &mut evidence {
        item.source_id = SourceId::new(source);
        item.source_item_key = SourceItemKey::new("meta");
    }
    let evidence_ids = evidence
        .iter()
        .map(|item| item.evidence_id.clone())
        .collect();
    GraphCandidate {
        candidate_id: id.to_string(),
        job_id: JobId::new(Uuid::from_u128(7)),
        source_id: SourceId::new(source),
        source_item_key: SourceItemKey::new("meta"),
        item_canonical_uri: "https://github.com/x/y".to_string(),
        document_id: None,
        kind: "repo_docs".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "github".to_string(),
            parser: None,
            version: "1".to_string(),
        },
        nodes: vec![
            node("repo", "https://github.com/x/y", "x/y"),
            node("docs_site", "https://x.dev/docs", "docs"),
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "repo_has_docs".to_string(),
            from_stable_key: "https://github.com/x/y".to_string(),
            to_stable_key: "https://x.dev/docs".to_string(),
            evidence_ids,
            properties: MetadataMap::new(),
        }],
        evidence,
        confidence: 0.8,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn shared_stage_hidden_replay_preserves_concurrent_source_and_is_idempotent() {
    let directory = std::env::temp_dir().join(format!("axon-stage-replay-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let live = SqliteGraphStore::connect(directory.join("live.sqlite").to_str().unwrap())
        .await
        .unwrap();
    live.upsert_candidates(vec![repo_docs_candidate(
        "a",
        "a",
        vec![ev("a", "sitemap", 0.4)],
    )])
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
    stage
        .write_candidates(vec![repo_docs_candidate(
            "b",
            "b",
            vec![ev("b", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_evidence")
        .fetch_one(live.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    live.upsert_candidates(vec![repo_docs_candidate(
        "c",
        "c",
        vec![ev("c", "sitemap", 0.6)],
    )])
    .await
    .unwrap();
    sqlx::query("UPDATE graph_edges SET metadata_json=json_set(metadata_json,'$.concurrent',1)")
        .execute(live.pool())
        .await
        .unwrap();
    let mut conn = live.pool().acquire().await.unwrap();
    stage.prepare_activation(&mut conn).await.unwrap();
    assert!(stage.write_candidates(vec![]).await.is_err());
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .unwrap();
    let result = stage.activate_in_tx(&mut conn).await.unwrap();
    sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .unwrap();
    assert_eq!(stage.activate_in_tx(&mut conn).await.unwrap(), result);
    sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_evidence")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 3);
    let sources: String = sqlx::query_scalar("SELECT source_ids_json FROM graph_nodes LIMIT 1")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert!(sources.contains("a") && sources.contains("b") && sources.contains("c"));
    let properties: String = sqlx::query_scalar("SELECT metadata_json FROM graph_edges LIMIT 1")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert!(properties.contains("concurrent"));
    stage.detach(&mut conn).await.unwrap();
    drop(conn);
    stage.mark_disposable().await.unwrap();
    GraphStage::reap(live.pool(), &[stage.id().to_string()])
        .await
        .unwrap();
    live.pool().close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn file_stage_bulk_activation_only_seeds_incoming_edges_and_disposes_safely() {
    let directory = std::env::temp_dir().join(format!("axon-stage-test-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let live = SqliteGraphStore::connect(directory.join("live.sqlite").to_str().unwrap())
        .await
        .unwrap();
    let first = repo_docs_candidate("a", "a", vec![ev("a", "sitemap", 0.4)]);
    let mut other = first.clone();
    other.candidate_id = "other".into();
    other.nodes[1].stable_key = "https://other.dev/docs".into();
    other.edges[0].to_stable_key = other.nodes[1].stable_key.clone();
    other.evidence[0].evidence_id = "other".into();
    other.edges[0].evidence_ids = vec!["other".into()];
    live.upsert_candidates(vec![first, other]).await.unwrap();
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
        .write_candidates(vec![repo_docs_candidate(
            "b",
            "b",
            vec![ev("b", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    let mut conn = live.pool().acquire().await.unwrap();
    stage.prepare_activation(&mut conn).await.unwrap();
    assert!(stage.write_candidates(vec![]).await.is_err());
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .unwrap();
    stage.activate_in_tx(&mut conn).await.unwrap();
    sqlx::query("COMMIT").execute(&mut *conn).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_evidence")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 3);
    stage.detach(&mut conn).await.unwrap();
    drop(conn);
    GraphStage::mark_disposable_ids(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
    assert_eq!(
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap(),
        0
    );
    stage.mark_disposable().await.unwrap();
    assert_eq!(
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .unwrap(),
        1
    );
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_stage_receipts")
        .fetch_one(live.pool())
        .await
        .unwrap();
    assert_eq!(receipts, 1);
    live.pool().close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn graph_stage_upgrade_from_five_migrations_keeps_existing_graph() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    for migration in &crate::migration::MIGRATIONS[..5] {
        sqlx::raw_sql(migration.sql).execute(&pool).await.unwrap();
    }
    let live = SqliteGraphStore::from_pool(pool.clone());
    live.upsert_candidates(vec![repo_docs_candidate(
        "old",
        "a",
        vec![ev("old", "sitemap", 0.4)],
    )])
    .await
    .unwrap();
    sqlx::raw_sql(crate::migration::MIGRATIONS[5].sql)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_nodes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let before: i64 = sqlx::query_scalar("SELECT revision FROM graph_revision")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE graph_evidence SET quote='changed'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE graph_aliases SET node_id=node_id")
        .execute(&pool)
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT revision FROM graph_revision")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(after > before);
    assert!(
        GraphStage::activation_summary(
            &pool,
            &SourceId::new("a"),
            &SourceGenerationId::new("none")
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn journal_admission_rejects_before_writing_and_disposal_refuses_outside_paths() {
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
    let path: String = sqlx::query_scalar("SELECT path FROM graph_stages WHERE stage_id=?")
        .bind(stage.id())
        .fetch_one(live.pool())
        .await
        .unwrap();
    let private = sqlx::SqlitePool::connect(&format!("sqlite://{path}"))
        .await
        .unwrap();
    sqlx::query("UPDATE stage_stats SET bytes=268435456")
        .execute(&private)
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
    assert!(error.message.contains("256 MiB"));
    assert_eq!(error.source_id.as_deref(), Some("b"));
    assert_eq!(
        error.details.get("stage_id").map(String::as_str),
        Some(stage.id())
    );
    assert!(!error.message.contains(&path));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM graph_nodes")
        .fetch_one(live.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    private.close().await;
    stage.mark_disposable().await.unwrap();
    let outside = std::env::temp_dir().join(format!("{}.sqlite", stage.id()));
    std::fs::write(&outside, b"preserve").unwrap();
    sqlx::query("UPDATE graph_stages SET path=? WHERE stage_id=?")
        .bind(outside.to_string_lossy().as_ref())
        .bind(stage.id())
        .execute(live.pool())
        .await
        .unwrap();
    assert!(
        GraphStage::reap(live.pool(), &[stage.id().into()])
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&outside).unwrap(), b"preserve");
    sqlx::query("UPDATE graph_stages SET path=? WHERE stage_id=?")
        .bind(&path)
        .bind(stage.id())
        .execute(live.pool())
        .await
        .unwrap();
    GraphStage::reap(live.pool(), &[stage.id().into()])
        .await
        .unwrap();
    std::fs::remove_file(outside).unwrap();
}

#[path = "stage_tests/lifecycle.rs"]
mod lifecycle;

#[path = "stage_tests/security.rs"]
mod security;

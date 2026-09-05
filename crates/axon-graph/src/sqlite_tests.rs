use super::*;
use crate::merge::{edge_id_for, node_id_for};
use axon_api::source::{
    AuthorityLevel, GraphCandidate, GraphCandidateProducer, GraphDirection, GraphEdgeCandidate,
    GraphEvidence, GraphIdentifier, GraphNodeCandidate, GraphNodeId, GraphQueryRequest,
    GraphResolveRequest, JobId, MetadataMap, SourceId, SourceItemKey,
};
use uuid::Uuid;

async fn store() -> SqliteGraphStore {
    SqliteGraphStore::connect(":memory:").await.unwrap()
}

#[tokio::test]
async fn standalone_schema_includes_publication_state() {
    let store = store().await;
    let table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'graph_publication_state'",
    )
    .fetch_optional(store.pool())
    .await
    .expect("inspect graph schema");
    assert_eq!(table.as_deref(), Some("graph_publication_state"));
}

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
async fn evidence_revision_replaces_all_lineage_and_content_fields() {
    let store = store().await;
    let first = repo_docs_candidate(
        "candidate",
        "source-a",
        vec![ev("evidence", "sitemap", 0.2)],
    );
    store.upsert_candidates(vec![first]).await.unwrap();

    let mut revised_evidence = ev("evidence", "github_homepage", 0.9);
    revised_evidence.source_item_key = SourceItemKey::new("revised-item");
    revised_evidence.quote = Some("revised quote".into());
    let revised = repo_docs_candidate("candidate-revision", "source-b", vec![revised_evidence]);
    store.upsert_candidates(vec![revised]).await.unwrap();

    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT evidence_kind, source_id, source_item_key, quote FROM graph_evidence WHERE evidence_id = ?",
    )
    .bind("evidence")
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "github_homepage".into(),
            "source-b".into(),
            "meta".into(),
            Some("revised quote".into())
        )
    );
}

#[tokio::test]
async fn mixed_source_batch_is_rejected_instead_of_misreporting_one_source() {
    let store = store().await;
    let result = store
        .upsert_candidates(vec![
            repo_docs_candidate(
                "candidate-a",
                "source-a",
                vec![ev("evidence-a", "sitemap", 0.8)],
            ),
            repo_docs_candidate(
                "candidate-b",
                "source-b",
                vec![ev("evidence-b", "sitemap", 0.8)],
            ),
        ])
        .await;
    let error = result.expect_err("mixed-source write needs per-source receipts");
    assert_eq!(error.code.to_string(), "graph.validation");
}

fn large_repo_candidate(item_count: usize) -> GraphCandidate {
    let mut nodes = vec![node("repo", "repo:root", "repo")];
    let mut edges = Vec::with_capacity(item_count);
    let mut evidence = Vec::with_capacity(item_count);
    for index in 0..item_count {
        let key = format!("src/file-{index:04}.rs");
        let evidence_id = format!("ev-{index:04}");
        nodes.push(node("repo_file", &key, &key));
        edges.push(GraphEdgeCandidate {
            edge_kind: "commit_contains_file".to_string(),
            from_stable_key: "repo:root".to_string(),
            to_stable_key: key,
            evidence_ids: vec![evidence_id.clone()],
            properties: MetadataMap::new(),
        });
        evidence.push(ev(&evidence_id, "text_mention", 0.95));
    }
    GraphCandidate {
        candidate_id: "gc-large".to_string(),
        job_id: JobId::new(Uuid::from_u128(9)),
        source_id: SourceId::new("src"),
        source_item_key: SourceItemKey::new("item"),
        item_canonical_uri: "https://github.com/x/large".to_string(),
        document_id: None,
        kind: "source_baseline".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "github".to_string(),
            parser: None,
            version: "1".to_string(),
        },
        nodes,
        edges,
        evidence,
        confidence: 0.95,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn large_upsert_crosses_alias_and_evidence_batch_boundaries() {
    let graph = store().await;
    let candidate = large_repo_candidate(121);
    let first = graph
        .upsert_candidates(vec![candidate.clone()])
        .await
        .expect("large graph upsert");
    assert_eq!(first.nodes_upserted, 122);
    assert_eq!(first.edges_upserted, 121);
    assert_eq!(first.evidence_records, 121);

    let aliases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_aliases")
        .fetch_one(graph.pool())
        .await
        .expect("count aliases");
    let evidence: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_evidence")
        .fetch_one(graph.pool())
        .await
        .expect("count evidence");
    assert_eq!(aliases, 122 * 3);
    assert_eq!(evidence, 121);

    graph
        .upsert_candidates(vec![candidate])
        .await
        .expect("idempotent large graph upsert");
    let evidence_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_evidence")
        .fetch_one(graph.pool())
        .await
        .expect("count evidence after replay");
    assert_eq!(evidence_after, 121);
}

#[tokio::test]
async fn ordinary_edge_upsert_is_bounded_past_sqlite_variable_limits() {
    let graph = store().await;
    let mut candidate = large_repo_candidate(1_001);
    candidate.kind = "repository_snapshot".to_string();

    let written = graph
        .upsert_candidates(vec![candidate.clone()])
        .await
        .expect("edge reads and writes must be split into bounded statements");
    assert_eq!(written.edges_upserted, 1_001);

    graph
        .upsert_candidates(vec![candidate])
        .await
        .expect("bounded replay must preserve merge behavior");
    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .expect("count graph edges");
    assert_eq!(edges, 1_001);
}

#[tokio::test]
async fn later_invalid_candidate_cannot_leave_earlier_candidate_batches_committed() {
    let graph = store().await;
    let mut valid = large_repo_candidate(201);
    valid.kind = "repository_snapshot".to_string();
    let mut invalid = repo_docs_candidate(
        "invalid-later-candidate",
        "src",
        vec![ev("invalid-evidence", "text_mention", 0.9)],
    );
    invalid.nodes[0].node_kind = "not_a_registered_node_kind".to_string();

    let error = graph
        .upsert_candidates(vec![valid, invalid])
        .await
        .expect_err("the complete caller batch must validate before any write");
    assert_eq!(error.code.to_string(), "graph.validation");

    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .expect("count graph edges after rejected batch");
    assert_eq!(
        edges, 0,
        "prevalidation failure must leave no committed prefix"
    );
}

#[tokio::test]
async fn later_edge_batch_failure_rolls_back_the_complete_candidate() {
    let graph = store().await;
    sqlx::query(
        "CREATE TRIGGER fail_later_edge_batch BEFORE INSERT ON graph_edges \
         WHEN (SELECT COUNT(*) FROM graph_edges) >= 100 \
         BEGIN SELECT RAISE(FAIL, 'injected later batch failure'); END",
    )
    .execute(graph.pool())
    .await
    .unwrap();
    let mut candidate = large_repo_candidate(201);
    candidate.kind = "repository_snapshot".to_string();

    let error = graph
        .upsert_candidates(vec![candidate.clone()])
        .await
        .unwrap_err();

    assert!(
        !error.message.contains("graph.partial_write"),
        "{}",
        error.message
    );
    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(
        edges, 0,
        "failed candidates must not leave a committed prefix"
    );
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_write_checkpoints")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(checkpoints, 0);
    sqlx::query("DROP TRIGGER fail_later_edge_batch")
        .execute(graph.pool())
        .await
        .unwrap();
    graph.upsert_candidates(vec![candidate]).await.unwrap();
    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(edges, 201);
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_write_checkpoints")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(checkpoints, 0);
}

#[tokio::test]
async fn small_candidate_never_touches_the_legacy_checkpoint_table() {
    let graph = store().await;
    sqlx::query(
        "CREATE TRIGGER reject_checkpoint_insert BEFORE INSERT ON graph_write_checkpoints \
         BEGIN SELECT RAISE(FAIL, 'checkpoint traffic is forbidden'); END",
    )
    .execute(graph.pool())
    .await
    .unwrap();

    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "small-no-checkpoint",
            "src",
            vec![ev("small-evidence", "text_mention", 0.9)],
        )])
        .await
        .expect("an atomic small candidate does not require checkpoint I/O");
}

#[tokio::test]
async fn stale_checkpoint_cannot_skip_edges_from_a_changed_candidate() {
    let graph = store().await;
    let mut candidate = large_repo_candidate(201);
    candidate.kind = "repository_snapshot".to_string();
    sqlx::query(
        "INSERT INTO graph_write_checkpoints \
         (job_id, candidate_id, next_edge_index, updated_at) VALUES (?, ?, 100, ?)",
    )
    .bind(candidate.job_id.0.to_string())
    .bind(&candidate.candidate_id)
    .bind("2026-09-04T00:00:00Z")
    .execute(graph.pool())
    .await
    .unwrap();

    graph.upsert_candidates(vec![candidate]).await.unwrap();

    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(
        edges, 201,
        "legacy checkpoint state must never suppress writes"
    );
    let checkpoints: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_write_checkpoints")
        .fetch_one(graph.pool())
        .await
        .unwrap();
    assert_eq!(
        checkpoints, 0,
        "successful reconciliation removes stale state"
    );
}

#[tokio::test]
async fn node_upsert_crosses_read_and_write_boundaries_without_losing_merges() {
    let graph = store().await;
    let mut first = large_repo_candidate(901);
    first.nodes[0]
        .properties
        .insert("api_key".to_string(), serde_json::json!("must-not-persist"));
    graph
        .upsert_candidates(vec![first.clone()])
        .await
        .expect("initial boundary upsert");

    let mut second = first;
    second.candidate_id = "gc-large-second-source".to_string();
    second.source_id = SourceId::new("src-second");
    for evidence in &mut second.evidence {
        evidence.source_id = SourceId::new("src-second");
    }
    graph
        .upsert_candidates(vec![second])
        .await
        .expect("merged boundary upsert");

    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(graph.pool())
        .await
        .expect("count nodes");
    let missing_sources: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_nodes WHERE source_ids_json NOT LIKE '%src%' OR source_ids_json NOT LIKE '%src-second%'",
    ).fetch_one(graph.pool()).await.expect("verify source union");
    assert_eq!(node_count, 902);
    assert_eq!(missing_sources, 0);
    let (authority, confidence, metadata): (String, f64, String) = sqlx::query_as(
        "SELECT authority, confidence, metadata_json FROM graph_nodes WHERE stable_key = 'repo:root'",
    )
    .fetch_one(graph.pool())
    .await
    .expect("inspect merged node");
    assert_eq!(authority, "inferred");
    assert!((confidence - 0.95).abs() < 1e-6);
    assert!(!metadata.contains("must-not-persist"));
}

fn repo_file_candidate(
    candidate_id: &str,
    candidate_kind: &str,
    source: &str,
    evidence_kind: &str,
) -> GraphCandidate {
    let mut evidence = ev("file-evidence", evidence_kind, 0.95);
    evidence.source_id = SourceId::new(source);
    GraphCandidate {
        candidate_id: candidate_id.to_string(),
        job_id: JobId::new(Uuid::from_u128(10)),
        source_id: SourceId::new(source),
        source_item_key: SourceItemKey::new("item"),
        item_canonical_uri: "https://github.com/x/y".to_string(),
        document_id: None,
        kind: candidate_kind.to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "github".to_string(),
            parser: None,
            version: "1".to_string(),
        },
        nodes: vec![
            node("repo", "repo:root", "repo"),
            node("repo_file", "src/lib.rs", "src/lib.rs"),
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "commit_contains_file".to_string(),
            from_stable_key: "repo:root".to_string(),
            to_stable_key: "src/lib.rs".to_string(),
            evidence_ids: vec![evidence.evidence_id.clone()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![evidence],
        confidence: 0.95,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn baseline_batch_preserves_higher_edge_authority_and_unions_node_sources() {
    let graph = store().await;
    let mut official = repo_file_candidate(
        "official",
        "repo_file_relation",
        "src-official",
        "github_homepage",
    );
    official.edges[0]
        .properties
        .insert("rank".to_string(), serde_json::json!("official"));
    graph
        .upsert_candidates(vec![official])
        .await
        .expect("official seed");

    let mut baseline = repo_file_candidate(
        "baseline",
        "source_baseline",
        "src-baseline",
        "text_mention",
    );
    baseline.edges[0]
        .properties
        .insert("rank".to_string(), serde_json::json!("baseline"));
    baseline.edges[0]
        .properties
        .insert("baseline_only".to_string(), serde_json::json!(true));
    graph
        .upsert_candidates(vec![baseline])
        .await
        .expect("batched baseline merge");

    let repo_id = node_id_for("repo", "repo:root");
    let file_id = node_id_for("repo_file", "src/lib.rs");
    let repo = graph.get_node(repo_id.clone()).await.unwrap().unwrap();
    assert!(repo.source_ids.contains(&SourceId::new("src-official")));
    assert!(repo.source_ids.contains(&SourceId::new("src-baseline")));

    let edge_id = edge_id_for("commit_contains_file", &repo_id, &file_id);
    let edge = graph.get_edge(edge_id).await.unwrap().unwrap();
    assert_eq!(edge.authority, AuthorityLevel::Official);
    assert_eq!(
        edge.metadata
            .get("rank")
            .and_then(serde_json::Value::as_str),
        Some("official")
    );
    assert_eq!(
        edge.metadata
            .get("baseline_only")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn upsert_then_get_node_and_edge_roundtrip() {
    let graph = store().await;
    let written = graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-1",
            "src",
            vec![ev("ev-1", "github_homepage", 0.95)],
        )])
        .await
        .unwrap();
    assert_eq!(written.candidates_seen, 1);
    assert_eq!(written.nodes_upserted, 2);
    assert_eq!(written.edges_upserted, 1);
    assert_eq!(written.evidence_records, 1);

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let fetched = graph.get_node(repo_id.clone()).await.unwrap().unwrap();
    assert_eq!(fetched.kind, "repo");
    assert_eq!(fetched.display_name, "x/y");
    assert_eq!(fetched.source_ids, vec![SourceId::new("src")]);

    let docs_id = node_id_for("docs_site", "https://x.dev/docs");
    let edge_id = edge_id_for("repo_has_docs", &repo_id, &docs_id);
    let edge = graph.get_edge(edge_id).await.unwrap().unwrap();
    assert_eq!(edge.kind, "repo_has_docs");
    // Official-authority evidence promotes the edge authority.
    assert_eq!(edge.authority, AuthorityLevel::Official);
    assert_eq!(edge.evidence.len(), 1);
    assert_eq!(edge.evidence[0].evidence_id, "ev-1");
    assert_eq!(edge.evidence[0].metadata["source_id"], "src");
    assert_eq!(edge.evidence[0].metadata["source_item_key"], "meta");
    assert_eq!(edge.evidence[0].metadata["redaction_status"], "clean");
    assert!(edge.evidence[0].metadata["redaction_version"].is_string());
    assert_eq!(edge.evidence[0].metadata["visibility"], "public");
}

#[tokio::test]
async fn upsert_redacts_secrets_from_node_and_edge_properties() {
    let graph = store().await;
    let mut secret_properties = MetadataMap::new();
    secret_properties.insert(
        "note".to_string(),
        serde_json::json!("authorization: bearer abcdef0123456789abcdef"),
    );
    let mut candidate = repo_docs_candidate("gc-secret", "src", vec![ev("ev-1", "sitemap", 0.5)]);
    candidate.nodes[0].properties = secret_properties.clone();
    candidate.edges[0].properties = secret_properties;
    graph.upsert_candidates(vec![candidate]).await.unwrap();

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let fetched_node = graph.get_node(repo_id.clone()).await.unwrap().unwrap();
    assert!(
        !fetched_node.metadata["note"]
            .as_str()
            .unwrap()
            .contains("abcdef0123456789abcdef")
    );

    let docs_id = node_id_for("docs_site", "https://x.dev/docs");
    let edge_id = edge_id_for("repo_has_docs", &repo_id, &docs_id);
    let fetched_edge = graph.get_edge(edge_id).await.unwrap().unwrap();
    assert!(
        !fetched_edge.metadata["note"]
            .as_str()
            .unwrap()
            .contains("abcdef0123456789abcdef")
    );
}

#[tokio::test]
async fn upsert_redacts_secrets_from_evidence_quote_and_metadata() {
    let graph = store().await;
    let mut evidence = ev("ev-secret", "sitemap", 0.5);
    evidence.quote = Some("Authorization: Bearer abcdef0123456789abcdef".to_string());
    evidence.metadata.insert(
        "note".to_string(),
        serde_json::json!("api key sk-proj-abcdefghijklmnopqrstuvwxyz0123456789"),
    );

    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-evidence-secret",
            "src",
            vec![evidence],
        )])
        .await
        .unwrap();

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let docs_id = node_id_for("docs_site", "https://x.dev/docs");
    let edge_id = edge_id_for("repo_has_docs", &repo_id, &docs_id);
    let fetched_edge = graph.get_edge(edge_id).await.unwrap().unwrap();
    let stored_evidence = fetched_edge
        .evidence
        .iter()
        .find(|evidence| evidence.evidence_id == "ev-secret")
        .expect("stored evidence");

    assert!(
        !stored_evidence
            .quote
            .as_deref()
            .unwrap_or_default()
            .contains("abcdef0123456789abcdef")
    );
    assert!(
        !stored_evidence.metadata["note"]
            .as_str()
            .unwrap()
            .contains("sk-proj-")
    );
}

#[tokio::test]
async fn upsert_is_idempotent_by_stable_key_and_tuple() {
    let graph = store().await;
    let cand = || repo_docs_candidate("gc", "src", vec![ev("ev-1", "github_homepage", 0.9)]);
    graph.upsert_candidates(vec![cand()]).await.unwrap();
    graph.upsert_candidates(vec![cand()]).await.unwrap();

    // Re-ingesting the same candidate must not duplicate nodes or edges.
    use sqlx::Row;
    let node_count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM graph_nodes")
        .fetch_one(graph.pool())
        .await
        .unwrap()
        .get("n");
    let edge_count: i64 = sqlx::query("SELECT COUNT(*) AS n FROM graph_edges")
        .fetch_one(graph.pool())
        .await
        .unwrap()
        .get("n");
    assert_eq!(node_count, 2);
    assert_eq!(edge_count, 1);
}

#[tokio::test]
async fn store_rejects_unknown_node_kind() {
    let graph = store().await;
    let mut cand = repo_docs_candidate("gc", "src", vec![ev("ev-1", "github_homepage", 0.9)]);
    cand.nodes[0].node_kind = "repository".to_string(); // forbidden alternate name
    let err = graph.upsert_candidates(vec![cand]).await.unwrap_err();
    assert!(
        err.message.contains("unknown graph node kind"),
        "{}",
        err.message
    );

    // Rejected batch must not have written anything.
    use sqlx::Row;
    let n: i64 = sqlx::query("SELECT COUNT(*) AS n FROM graph_nodes")
        .fetch_one(graph.pool())
        .await
        .unwrap()
        .get("n");
    assert_eq!(n, 0);
}

#[tokio::test]
async fn store_rejects_unknown_edge_kind() {
    let graph = store().await;
    let mut cand = repo_docs_candidate("gc", "src", vec![ev("ev-1", "github_homepage", 0.9)]);
    cand.edges[0].edge_kind = "links_to".to_string();
    let err = graph.upsert_candidates(vec![cand]).await.unwrap_err();
    assert!(
        err.message.contains("unknown graph edge kind"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn conflicting_official_claims_are_recorded_not_overwritten() {
    let graph = store().await;
    // First official claim.
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-1",
            "src-a",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    // Second official claim of equal rank from a different source → conflict.
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-2",
            "src-b",
            vec![ev("ev-2", "package_repository", 0.9)],
        )])
        .await
        .unwrap();

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let docs_id = node_id_for("docs_site", "https://x.dev/docs");
    let edge_id = edge_id_for("repo_has_docs", &repo_id, &docs_id);

    // The edge is marked conflicting (not silently kept as one official claim).
    let edge = graph.get_edge(edge_id.clone()).await.unwrap().unwrap();
    assert_eq!(edge.authority, AuthorityLevel::Conflicting);
    // Both evidence records are preserved.
    assert_eq!(edge.evidence.len(), 2);
    // An explicit conflict row was recorded.
    assert_eq!(graph.edge_conflict_count(&edge_id.0).await.unwrap(), 1);
}

#[tokio::test]
async fn higher_authority_claim_wins_without_conflict() {
    let graph = store().await;
    // Inferred claim first.
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-1",
            "src-a",
            vec![ev("ev-1", "sitemap", 0.5)],
        )])
        .await
        .unwrap();
    // User-pinned claim second → strictly higher, wins, no conflict.
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-2",
            "src-b",
            vec![ev("ev-2", "user_pinned", 0.99)],
        )])
        .await
        .unwrap();

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let docs_id = node_id_for("docs_site", "https://x.dev/docs");
    let edge_id = edge_id_for("repo_has_docs", &repo_id, &docs_id);
    let edge = graph.get_edge(edge_id.clone()).await.unwrap().unwrap();
    assert_eq!(edge.authority, AuthorityLevel::UserPinned);
    assert_eq!(graph.edge_conflict_count(&edge_id.0).await.unwrap(), 0);
}

#[tokio::test]
async fn resolve_finds_node_by_stable_key_canonical_uri_and_node_id() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc",
            "src",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    let repo_id = node_id_for("repo", "https://github.com/x/y");

    // By stable key (identifier.value).
    let by_key = graph
        .resolve(GraphResolveRequest {
            identifiers: vec![GraphIdentifier {
                kind: "repo".to_string(),
                canonical_uri: None,
                value: Some("https://github.com/x/y".to_string()),
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            }],
            include_edges: true,
        })
        .await
        .unwrap();
    assert_eq!(by_key.resolved.len(), 1);
    assert_eq!(by_key.misses.len(), 0);
    assert_eq!(by_key.resolved[0].node.node_id, repo_id);
    assert_eq!(by_key.resolved[0].edges.len(), 1);

    // By node id.
    let by_id = graph
        .resolve(GraphResolveRequest {
            identifiers: vec![GraphIdentifier {
                kind: "repo".to_string(),
                canonical_uri: None,
                value: None,
                node_id: Some(repo_id.clone()),
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            }],
            include_edges: false,
        })
        .await
        .unwrap();
    assert_eq!(by_id.resolved.len(), 1);

    // A miss is reported explicitly.
    let miss = graph
        .resolve(GraphResolveRequest {
            identifiers: vec![GraphIdentifier {
                kind: "repo".to_string(),
                canonical_uri: None,
                value: Some("nope".to_string()),
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            }],
            include_edges: false,
        })
        .await
        .unwrap();
    assert_eq!(miss.resolved.len(), 0);
    assert_eq!(miss.misses.len(), 1);
}

#[tokio::test]
async fn query_traverses_outbound_with_depth_and_edge_filter() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc",
            "src",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();

    let start = GraphIdentifier {
        kind: "repo".to_string(),
        canonical_uri: None,
        value: Some("https://github.com/x/y".to_string()),
        node_id: None,
        source_id: None,
        source_item_key: None,
        metadata: MetadataMap::new(),
    };

    let out = graph
        .query(GraphQueryRequest {
            start: start.clone(),
            edges: vec!["repo_has_docs".to_string()],
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(out.nodes.len(), 2); // repo + docs_site
    assert_eq!(out.edges.len(), 1);
    assert_eq!(out.evidence.len(), 1);

    // Depth 0 returns only the start node, no edges.
    let d0 = graph
        .query(GraphQueryRequest {
            start: start.clone(),
            edges: vec![],
            direction: GraphDirection::Out,
            depth: 0,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(d0.nodes.len(), 1);
    assert!(d0.edges.is_empty());

    // A non-matching edge filter yields no edges.
    let filtered = graph
        .query(GraphQueryRequest {
            start,
            edges: vec!["repo_has_wiki".to_string()],
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert!(filtered.edges.is_empty());
}

#[tokio::test]
async fn query_rejects_work_above_hard_depth_and_edge_budgets() {
    let graph = store().await;
    let request = |depth, limit| GraphQueryRequest {
        start: GraphIdentifier {
            kind: "repo".to_string(),
            canonical_uri: None,
            value: Some("https://github.com/x/y".to_string()),
            node_id: None,
            source_id: None,
            source_item_key: None,
            metadata: MetadataMap::new(),
        },
        edges: Vec::new(),
        direction: GraphDirection::Both,
        depth,
        filters: None,
        limit,
        cursor: None,
    };

    let depth = graph.query(request(9, 10)).await.unwrap_err();
    assert_eq!(depth.code.to_string(), "graph.depth_limit_exceeded");
    let edges = graph.query(request(1, 1_001)).await.unwrap_err();
    assert_eq!(edges.code.to_string(), "graph.edge_limit_exceeded");
}

#[tokio::test]
async fn query_cursor_returns_stable_non_overlapping_pages() {
    let graph = store().await;
    let mut candidate = repo_docs_candidate(
        "gc-page",
        "src",
        vec![
            ev("ev-page-1", "github_homepage", 0.9),
            ev("ev-page-2", "github_homepage", 0.8),
        ],
    );
    candidate
        .nodes
        .push(node("web_page", "https://github.com/x/y/wiki", "wiki"));
    candidate.edges.push(GraphEdgeCandidate {
        edge_kind: "repo_has_wiki".to_string(),
        from_stable_key: "https://github.com/x/y".to_string(),
        to_stable_key: "https://github.com/x/y/wiki".to_string(),
        evidence_ids: vec!["ev-page-2".to_string()],
        properties: MetadataMap::new(),
    });
    graph.upsert_candidates(vec![candidate]).await.unwrap();
    let request = |cursor| GraphQueryRequest {
        start: GraphIdentifier {
            kind: "repo".to_string(),
            canonical_uri: None,
            value: Some("https://github.com/x/y".to_string()),
            node_id: None,
            source_id: None,
            source_item_key: None,
            metadata: MetadataMap::new(),
        },
        edges: Vec::new(),
        direction: GraphDirection::Out,
        depth: 1,
        filters: None,
        limit: 1,
        cursor,
    };

    let first = graph.query(request(None)).await.unwrap();
    assert_eq!(first.edges.len(), 1);
    let cursor = first
        .next_cursor
        .clone()
        .expect("first page should advertise the second page");
    let second = graph.query(request(Some(cursor))).await.unwrap();
    assert_eq!(second.edges.len(), 1);
    assert_ne!(first.edges[0].edge_id, second.edges[0].edge_id);
    assert_eq!(second.next_cursor, None);

    let error = graph
        .query(request(Some("edge_missing".to_string())))
        .await
        .unwrap_err();
    assert_eq!(error.code.to_string(), "graph.invalid_cursor");
}

#[tokio::test]
async fn query_inbound_direction_from_leaf_finds_parent() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc",
            "src",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();

    let inbound = graph
        .query(GraphQueryRequest {
            start: GraphIdentifier {
                kind: "docs_site".to_string(),
                canonical_uri: Some("https://x.dev/docs".to_string()),
                value: None,
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            },
            edges: vec![],
            direction: GraphDirection::In,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(inbound.edges.len(), 1);
    let repo_id = node_id_for("repo", "https://github.com/x/y");
    assert!(inbound.nodes.iter().any(|n| n.node_id == repo_id));
}

#[tokio::test]
async fn reset_clears_all_tables() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc",
            "src",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    graph.reset().await.unwrap();
    let repo_id = node_id_for("repo", "https://github.com/x/y");
    assert!(graph.get_node(repo_id).await.unwrap().is_none());
    let cap = graph.capabilities().await.unwrap();
    assert_eq!(cap.0.owner_crate, "axon-graph");
    assert_eq!(cap.0.name, "sqlite-graph");
}

#[tokio::test]
async fn multi_source_upsert_unions_node_source_ids() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-1",
            "src-a",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-2",
            "src-b",
            vec![ev("ev-2", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    let repo_id: GraphNodeId = node_id_for("repo", "https://github.com/x/y");
    let node = graph.get_node(repo_id).await.unwrap().unwrap();
    assert_eq!(node.source_ids.len(), 2);
    assert!(node.source_ids.contains(&SourceId::new("src-a")));
    assert!(node.source_ids.contains(&SourceId::new("src-b")));
}

#[test]
fn node_source_membership_uses_a_set_without_changing_serialized_order() {
    let source = include_str!("sqlite/upsert/nodes.rs");
    assert!(source.contains("source_id_set: HashSet<String>"));
    assert!(source.contains("source_ids: Vec<SourceId>"));
    assert!(!source.contains("state.source_ids.contains(source_id)"));
}

#[tokio::test]
async fn node_edges_returns_incident_edges_regardless_of_direction() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc",
            "src",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();

    let repo_id = node_id_for("repo", "https://github.com/x/y");
    let docs_id = node_id_for("docs_site", "https://x.dev/docs");

    let repo_edges = graph.node_edges(repo_id).await.unwrap();
    assert_eq!(repo_edges.len(), 1);
    assert_eq!(repo_edges[0].kind, "repo_has_docs");

    let docs_edges = graph.node_edges(docs_id).await.unwrap();
    assert_eq!(
        docs_edges.len(),
        1,
        "docs_site is the `to` side of the edge"
    );

    let none = graph.node_edges(GraphNodeId::new("missing")).await.unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn nodes_for_source_filters_by_source_id_without_prefix_collisions() {
    let graph = store().await;
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-1",
            "src-a",
            vec![ev("ev-1", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();
    graph
        .upsert_candidates(vec![repo_docs_candidate(
            "gc-2",
            "src-ab",
            vec![ev("ev-2", "github_homepage", 0.9)],
        )])
        .await
        .unwrap();

    // Both candidates upsert onto the SAME nodes (repo/docs_site stable keys
    // are identical), so both source ids land on both nodes' `source_ids`.
    let nodes = graph
        .nodes_for_source(SourceId::new("src-a"))
        .await
        .unwrap();
    assert_eq!(nodes.len(), 2);
    assert!(
        nodes
            .iter()
            .all(|node| node.source_ids.contains(&SourceId::new("src-a")))
    );

    let none = graph
        .nodes_for_source(SourceId::new("src-missing"))
        .await
        .unwrap();
    assert!(none.is_empty());
}

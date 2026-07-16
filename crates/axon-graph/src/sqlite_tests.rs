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
            properties: MetadataMap::new(),
        }],
        evidence,
        confidence: 0.8,
        metadata: MetadataMap::new(),
    }
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

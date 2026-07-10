use super::*;
use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    JobId, MetadataMap, SourceItemKey,
};
use axon_graph::store::FakeGraphStore;
use uuid::Uuid;

fn candidate(source: &str, candidate_id: &str) -> GraphCandidate {
    GraphCandidate {
        candidate_id: candidate_id.to_string(),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new(source),
        source_item_key: SourceItemKey::new("Cargo.toml"),
        item_canonical_uri: "file:///repo/Cargo.toml".to_string(),
        document_id: None,
        kind: "manifest_dependency".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "local".to_string(),
            parser: Some("manifest".to_string()),
            version: "test".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "package".to_string(),
                stable_key: "pkg:axon".to_string(),
                label: "axon".to_string(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "package".to_string(),
                stable_key: "pkg:tokio".to_string(),
                label: "tokio".to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "package_has_repo".to_string(),
            from_stable_key: "pkg:axon".to_string(),
            to_stable_key: "pkg:tokio".to_string(),
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!("ev-{candidate_id}"),
            evidence_kind: "dependency_manifest".to_string(),
            source_id: SourceId::new(source),
            source_item_key: SourceItemKey::new("Cargo.toml"),
            document_id: None,
            chunk_id: None,
            range: None,
            quote: Some("tokio = \"1\"".to_string()),
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

#[test]
fn kinds_reports_nonempty_closed_registries_and_all_authority_levels() {
    let doc = kinds();
    assert!(!doc.node_kinds.is_empty());
    assert!(!doc.edge_kinds.is_empty());
    assert!(!doc.evidence_kinds.is_empty());
    assert_eq!(doc.authority_levels.len(), 8);
    assert!(doc.node_kinds.iter().any(|k| k == "repo"));
    assert!(doc.edge_kinds.iter().any(|k| k == "repo_has_docs"));
}

#[tokio::test]
async fn node_detail_returns_none_for_missing_node() {
    let store = FakeGraphStore::new();
    let found = node_detail(&store, GraphNodeId::new("missing"), true)
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn node_detail_includes_edges_only_when_requested() {
    let store = FakeGraphStore::new();
    store
        .upsert_candidates(vec![candidate("src_a", "c1")])
        .await
        .unwrap();

    let bare = node_detail(&store, GraphNodeId::new("pkg:axon"), false)
        .await
        .unwrap()
        .expect("node exists");
    assert!(bare.edges.is_empty());

    let with_edges = node_detail(&store, GraphNodeId::new("pkg:axon"), true)
        .await
        .unwrap()
        .expect("node exists");
    assert_eq!(with_edges.edges.len(), 1);
    assert_eq!(with_edges.node.node_id, GraphNodeId::new("pkg:axon"));
}

#[tokio::test]
async fn source_subgraph_returns_empty_for_unknown_source() {
    let store = FakeGraphStore::new();
    store
        .upsert_candidates(vec![candidate("src_a", "c1")])
        .await
        .unwrap();

    let result = source_subgraph(&store, SourceId::new("src_missing"), 1, None, 100)
        .await
        .unwrap();
    assert!(result.nodes.is_empty());
    assert!(result.edges.is_empty());
}

#[tokio::test]
async fn source_subgraph_depth_zero_returns_direct_nodes_without_expansion() {
    let store = FakeGraphStore::new();
    store
        .upsert_candidates(vec![candidate("src_a", "c1")])
        .await
        .unwrap();

    let result = source_subgraph(&store, SourceId::new("src_a"), 0, None, 100)
        .await
        .unwrap();
    assert_eq!(result.nodes.len(), 2);
    assert!(result.edges.is_empty());
}

#[tokio::test]
async fn source_subgraph_expands_edges_within_depth() {
    let store = FakeGraphStore::new();
    store
        .upsert_candidates(vec![candidate("src_a", "c1")])
        .await
        .unwrap();

    let result = source_subgraph(&store, SourceId::new("src_a"), 1, None, 100)
        .await
        .unwrap();
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].kind, "package_has_repo");
}

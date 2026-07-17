use axon_api::source::*;
use uuid::Uuid;

use crate::store::{FakeGraphStore, GraphStore};

fn candidate() -> GraphCandidate {
    GraphCandidate {
        candidate_id: "cand-a".to_string(),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new("src_a"),
        source_item_key: SourceItemKey::new("Cargo.toml"),
        item_canonical_uri: "file:///repo/Cargo.toml".to_string(),
        document_id: Some(DocumentId::new("doc-a")),
        kind: "manifest_dependency".to_string(),
        merge_key: Some("manifest:file:///repo/Cargo.toml".to_string()),
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
                node_kind: "dependency".to_string(),
                stable_key: "crate:tokio".to_string(),
                label: "tokio".to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "depends_on".to_string(),
            from_stable_key: "pkg:axon".to_string(),
            to_stable_key: "crate:tokio".to_string(),
            evidence_ids: vec!["ev-a".to_string()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: "ev-a".to_string(),
            evidence_kind: "manifest_range".to_string(),
            source_id: SourceId::new("src_a"),
            source_item_key: SourceItemKey::new("Cargo.toml"),
            document_id: Some(DocumentId::new("doc-a")),
            chunk_id: None,
            range: None,
            quote: Some("tokio = ...".to_string()),
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

fn multi_edge_candidate() -> GraphCandidate {
    let mut candidate = candidate();
    candidate.candidate_id = "cand-b".to_string();
    candidate.nodes.push(GraphNodeCandidate {
        node_kind: "module".to_string(),
        stable_key: "module:store".to_string(),
        label: "store".to_string(),
        properties: MetadataMap::new(),
    });
    candidate.edges.push(GraphEdgeCandidate {
        edge_kind: "contains".to_string(),
        from_stable_key: "pkg:axon".to_string(),
        to_stable_key: "module:store".to_string(),
        evidence_ids: vec!["ev-a".to_string()],
        properties: MetadataMap::new(),
    });
    candidate
}

fn graph_identifier(value: &str) -> GraphIdentifier {
    GraphIdentifier {
        kind: "package".to_string(),
        canonical_uri: None,
        value: Some(value.to_string()),
        node_id: None,
        source_id: None,
        source_item_key: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn fake_graph_store_upserts_candidates_and_resolves_nodes() {
    let graph = FakeGraphStore::new();
    let written = graph
        .upsert_candidates(vec![candidate(), multi_edge_candidate()])
        .await
        .unwrap();
    assert_eq!(written.source_id, SourceId::new("src_a"));
    assert_eq!(written.candidates_seen, 2);
    assert_eq!(written.nodes_upserted, 3);
    assert_eq!(written.edges_upserted, 2);
    assert_eq!(written.evidence_records, 2);

    let resolved = graph
        .resolve(GraphResolveRequest {
            identifiers: vec![graph_identifier("pkg:axon")],
            include_edges: true,
        })
        .await
        .unwrap();
    assert_eq!(resolved.resolved.len(), 1);
    assert_eq!(resolved.resolved[0].edges.len(), 2);
}

#[tokio::test]
async fn fake_graph_store_queries_and_reports_capabilities() {
    let graph = FakeGraphStore::new();
    graph.upsert_candidates(vec![candidate()]).await.unwrap();

    let result = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: vec!["depends_on".to_string()],
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.edges.len(), 1);

    let capability = graph.capabilities().await.unwrap();
    assert_eq!(capability.0.owner_crate, "axon-graph");

    graph.reset().await.unwrap();
    let empty = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: Vec::new(),
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert!(empty.nodes.is_empty());
}

#[tokio::test]
async fn fake_graph_store_honors_direction_and_depth() {
    let graph = FakeGraphStore::new();
    graph
        .upsert_candidates(vec![multi_edge_candidate()])
        .await
        .unwrap();

    let inbound = graph
        .query(GraphQueryRequest {
            start: graph_identifier("crate:tokio"),
            edges: vec!["depends_on".to_string()],
            direction: GraphDirection::In,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(inbound.edges.len(), 1);
    assert!(
        inbound
            .nodes
            .iter()
            .any(|node| node.node_id == GraphNodeId::new("pkg:axon"))
    );

    let depth_zero = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: Vec::new(),
            direction: GraphDirection::Out,
            depth: 0,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(depth_zero.nodes.len(), 1);
    assert!(depth_zero.edges.is_empty());

    let both = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: Vec::new(),
            direction: GraphDirection::Both,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(both.edges.len(), 2);
    assert_eq!(both.nodes.len(), 3);
}

#[tokio::test]
async fn fake_graph_store_merges_evidence_for_existing_edge() {
    let graph = FakeGraphStore::new();
    let mut first = candidate();
    first.evidence[0].evidence_id = "ev-first".to_string();
    first.edges[0].evidence_ids = vec!["ev-first".to_string()];
    let mut second = candidate();
    second.candidate_id = "cand-second".to_string();
    second.evidence[0].evidence_id = "ev-second".to_string();
    second.edges[0].evidence_ids = vec!["ev-second".to_string()];
    second.evidence[0].quote = Some("tokio = { version = \"1\" }".to_string());

    graph.upsert_candidates(vec![first]).await.unwrap();
    graph.upsert_candidates(vec![second]).await.unwrap();

    let result = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: vec!["depends_on".to_string()],
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();

    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.evidence.len(), 2);
    assert!(
        result
            .evidence
            .iter()
            .any(|evidence| evidence.evidence_id == "ev-second")
    );
}

#[tokio::test]
async fn fake_graph_store_warns_on_node_kind_conflict() {
    let graph = FakeGraphStore::new();
    let mut first = candidate();
    first.nodes[0].node_kind = "package".to_string();
    let mut conflicting = candidate();
    conflicting.candidate_id = "cand-conflict".to_string();
    conflicting.nodes[0].node_kind = "service".to_string();

    graph.upsert_candidates(vec![first]).await.unwrap();
    let written = graph.upsert_candidates(vec![conflicting]).await.unwrap();

    assert!(
        written
            .warnings
            .iter()
            .any(|warning| warning.code == "graph.node_kind_conflict"),
        "graph fake should expose conflicts instead of silently replacing node identity"
    );
}

#[tokio::test]
async fn fake_graph_store_node_edges_returns_incident_edges_both_directions() {
    let graph = FakeGraphStore::new();
    graph
        .upsert_candidates(vec![multi_edge_candidate()])
        .await
        .unwrap();

    let edges = graph
        .node_edges(GraphNodeId::new("pkg:axon"))
        .await
        .unwrap();
    assert_eq!(edges.len(), 2, "pkg:axon is the `from` side of both edges");

    let tokio_edges = graph
        .node_edges(GraphNodeId::new("crate:tokio"))
        .await
        .unwrap();
    assert_eq!(tokio_edges.len(), 1);

    let none = graph
        .node_edges(GraphNodeId::new("does-not-exist"))
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn fake_graph_store_nodes_for_source_filters_by_source_id() {
    let graph = FakeGraphStore::new();
    graph.upsert_candidates(vec![candidate()]).await.unwrap();

    let nodes = graph
        .nodes_for_source(SourceId::new("src_a"))
        .await
        .unwrap();
    assert_eq!(nodes.len(), 2, "both nodes came from source src_a");

    let none = graph
        .nodes_for_source(SourceId::new("src_missing"))
        .await
        .unwrap();
    assert!(none.is_empty());
}

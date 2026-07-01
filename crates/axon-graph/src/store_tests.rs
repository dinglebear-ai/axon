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
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: "ev-a".to_string(),
            source_id: SourceId::new("src_a"),
            source_item_key: SourceItemKey::new("Cargo.toml"),
            document_id: Some(DocumentId::new("doc-a")),
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
    assert_eq!(written.nodes_upserted, 5);
    assert_eq!(written.edges_upserted, 3);
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

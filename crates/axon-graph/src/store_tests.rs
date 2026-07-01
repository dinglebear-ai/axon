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

#[tokio::test]
async fn fake_graph_store_upserts_candidates_and_resolves_nodes() {
    let graph = FakeGraphStore::new();
    let written = graph.upsert_candidates(vec![candidate()]).await.unwrap();
    assert_eq!(written.nodes_upserted, 2);
    assert_eq!(written.edges_upserted, 1);
    assert_eq!(written.evidence_records, 1);

    let resolved = graph
        .resolve(GraphResolveRequest {
            identifiers: vec![GraphIdentifier {
                kind: "package".to_string(),
                canonical_uri: None,
                value: Some("pkg:axon".to_string()),
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            }],
            include_edges: true,
        })
        .await
        .unwrap();
    assert_eq!(resolved.resolved.len(), 1);
    assert_eq!(resolved.resolved[0].edges.len(), 1);
}

#[tokio::test]
async fn fake_graph_store_queries_and_reports_capabilities() {
    let graph = FakeGraphStore::new();
    graph.upsert_candidates(vec![candidate()]).await.unwrap();

    let result = graph
        .query(GraphQueryRequest {
            start: GraphIdentifier {
                kind: "package".to_string(),
                canonical_uri: None,
                value: Some("pkg:axon".to_string()),
                node_id: None,
                source_id: None,
                source_item_key: None,
                metadata: MetadataMap::new(),
            },
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
}

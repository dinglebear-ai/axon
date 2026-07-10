use std::sync::Arc;

use super::*;

/// Compile-level assertion that the production impl satisfies the trait
/// object (no live providers exercised).
#[allow(dead_code)]
fn assert_graph_service_impl_object_safe(ctx: Arc<ServiceContext>) -> Arc<dyn GraphService> {
    Arc::new(GraphServiceImpl::new(ctx))
}

fn sample_edge(id: &str, from: &str, to: &str) -> GraphEdge {
    GraphEdge {
        edge_id: GraphEdgeId::new(id),
        kind: "relates_to".to_string(),
        from_node_id: GraphNodeId::new(from),
        to_node_id: GraphNodeId::new(to),
        authority: axon_api::source::AuthorityLevel::Inferred,
        confidence: 0.8,
        evidence: Vec::new(),
        metadata: axon_api::source::MetadataMap::new(),
    }
}

fn sample_node(id: &str) -> GraphNode {
    GraphNode {
        node_id: GraphNodeId::new(id),
        kind: "entity".to_string(),
        canonical_uri: format!("urn:fake:{id}"),
        display_name: id.to_string(),
        authority: axon_api::source::AuthorityLevel::Inferred,
        confidence: 0.8,
        metadata: axon_api::source::MetadataMap::new(),
        source_ids: Vec::new(),
        created_at: None,
        updated_at: None,
    }
}

#[tokio::test]
async fn fake_graph_service_get_node_after_seed() {
    let fake = FakeGraphService::new();
    fake.seed_node(sample_node("node-1"));
    let node = fake
        .get_node(GraphNodeId::new("node-1"))
        .await
        .expect("node should exist");
    assert_eq!(node.node_id.0, "node-1");
}

#[tokio::test]
async fn fake_graph_service_resolve_reports_misses() {
    let fake = FakeGraphService::new();
    let request = GraphResolveRequest {
        identifiers: vec![axon_api::source::GraphIdentifier {
            kind: "entity".to_string(),
            canonical_uri: None,
            value: None,
            node_id: Some(GraphNodeId::new("missing")),
            source_id: None,
            source_item_key: None,
            metadata: axon_api::source::MetadataMap::new(),
        }],
        include_edges: false,
    };
    let result = fake.resolve(request).await.expect("resolve should succeed");
    assert_eq!(result.resolved.len(), 0);
    assert_eq!(result.misses.len(), 1);
}

#[tokio::test]
async fn fake_graph_service_kinds_through_trait_object() {
    let fake: Arc<dyn GraphService> = Arc::new(FakeGraphService::new());
    let kinds = fake.kinds().await.expect("kinds should succeed");
    assert!(kinds.node_kinds.contains(&"entity".to_string()));
    assert!(kinds.edge_kinds.contains(&"relates_to".to_string()));
}

#[tokio::test]
async fn fake_graph_service_query_returns_seeded_nodes() {
    let fake = FakeGraphService::new();
    fake.seed_node(sample_node("node-1"));
    fake.seed_node(sample_node("node-2"));

    let request = GraphQueryRequest {
        start: axon_api::source::GraphIdentifier {
            kind: "entity".to_string(),
            canonical_uri: None,
            value: None,
            node_id: Some(GraphNodeId::new("node-1")),
            source_id: None,
            source_item_key: None,
            metadata: axon_api::source::MetadataMap::new(),
        },
        edges: Vec::new(),
        direction: axon_api::source::GraphDirection::Both,
        depth: 1,
        filters: None,
        limit: 10,
        cursor: None,
    };
    let result = fake.query(request).await.expect("query should succeed");
    assert_eq!(result.nodes.len(), 2);
    assert!(result.next_cursor.is_none());
}

#[tokio::test]
async fn fake_graph_service_get_edge_after_seed() {
    let fake: Arc<dyn GraphService> = Arc::new(FakeGraphService::new());
    // Downcast not available on trait objects; seed via a concrete fake
    // instance instead, matching the get_node/resolve test pattern above.
    let concrete = FakeGraphService::new();
    concrete.seed_edge(sample_edge("edge-1", "node-1", "node-2"));
    let seeded: Arc<dyn GraphService> = Arc::new(concrete);

    let missing = fake.get_edge(GraphEdgeId::new("edge-1")).await;
    assert!(missing.is_err());

    let edge = seeded
        .get_edge(GraphEdgeId::new("edge-1"))
        .await
        .expect("edge should exist");
    assert_eq!(edge.edge_id.0, "edge-1");
    assert_eq!(edge.from_node_id.0, "node-1");
    assert_eq!(edge.to_node_id.0, "node-2");
}

use chrono::Utc;

use super::*;

#[test]
fn graph_query_and_resolve_dtos_round_trip() {
    let node_id = GraphNodeId::from("node_1");
    let edge_id = GraphEdgeId::from("edge_1");
    let identifier = GraphIdentifier {
        kind: "package".to_string(),
        canonical_uri: Some("pkg:npm/@example/pkg".to_string()),
        value: Some("@example/pkg".to_string()),
        node_id: Some(node_id.clone()),
        source_id: Some(SourceId::from("src_1")),
        source_item_key: Some(SourceItemKey::from("package.json")),
        metadata: MetadataMap::default(),
    };
    let evidence = GraphEvidence {
        evidence_id: "ev_1".to_string(),
        evidence_kind: "manifest_dependency".to_string(),
        source_id: SourceId::from("src_1"),
        source_item_key: SourceItemKey::from("package.json"),
        document_id: Some(DocumentId::from("doc_1")),
        chunk_id: None,
        range: None,
        quote: Some("\"@example/pkg\"".to_string()),
        confidence: 0.95,
        metadata: MetadataMap::default(),
    };
    let node = GraphNode {
        node_id: node_id.clone(),
        kind: "package".to_string(),
        canonical_uri: "pkg:npm/@example/pkg".to_string(),
        display_name: "@example/pkg".to_string(),
        authority: AuthorityLevel::Inferred,
        confidence: 0.95,
        metadata: MetadataMap::default(),
        source_ids: vec![SourceId::from("src_1")],
        created_at: Some(Timestamp::from(Utc::now())),
        updated_at: None,
    };
    let edge = GraphEdge {
        edge_id,
        kind: "depends_on".to_string(),
        from_node_id: GraphNodeId::from("node_project"),
        to_node_id: node_id,
        authority: AuthorityLevel::Inferred,
        confidence: 0.91,
        evidence: vec![evidence.clone()],
        metadata: MetadataMap::default(),
    };
    let resolve = GraphResolveResult {
        resolved: vec![GraphResolveMatch {
            identifier: identifier.clone(),
            node: node.clone(),
            confidence: 0.95,
            evidence: vec![evidence.clone()],
            edges: vec![edge.clone()],
        }],
        misses: Vec::new(),
        warnings: Vec::new(),
    };
    let query = GraphQueryResult {
        nodes: vec![node],
        edges: vec![edge],
        evidence: vec![evidence],
        next_cursor: None,
        warnings: Vec::new(),
    };
    let request = GraphQueryRequest {
        start: identifier,
        edges: vec!["depends_on".to_string()],
        direction: GraphDirection::Out,
        depth: 1,
        filters: Some(GraphQueryFilters {
            node_kinds: vec!["package".to_string()],
            source_ids: vec![SourceId::from("src_1")],
            min_confidence: Some(0.75),
            metadata: MetadataMap::default(),
        }),
        limit: 100,
        cursor: Some("cursor_1".to_string()),
    };
    let kinds = GraphKindDocument {
        node_kinds: vec!["package".to_string()],
        edge_kinds: vec!["depends_on".to_string()],
        evidence_kinds: vec!["manifest".to_string()],
        authority_levels: vec![AuthorityLevel::Official, AuthorityLevel::Inferred],
    };

    assert_eq!(
        serde_json::from_value::<GraphResolveResult>(serde_json::to_value(&resolve).unwrap())
            .unwrap(),
        resolve
    );
    assert_eq!(
        serde_json::from_value::<GraphQueryResult>(serde_json::to_value(&query).unwrap()).unwrap(),
        query
    );
    assert_eq!(
        serde_json::from_value::<GraphQueryRequest>(serde_json::to_value(&request).unwrap())
            .unwrap(),
        request
    );
    assert_eq!(
        serde_json::from_value::<GraphKindDocument>(serde_json::to_value(&kinds).unwrap()).unwrap(),
        kinds
    );
}

#[test]
fn graph_requests_reject_unknown_fields() {
    let bad = serde_json::json!({
        "identifiers": [],
        "include_edges": true,
        "legacy": true
    });

    assert!(serde_json::from_value::<GraphResolveRequest>(bad).is_err());

    let bad = serde_json::json!({
        "start": {
            "kind": "repo",
            "canonical_uri": "https://github.com/jmagar/axon",
            "metadata": {}
        },
        "direction": "sideways",
        "depth": 1,
        "limit": 100
    });

    let err = serde_json::from_value::<GraphQueryRequest>(bad)
        .expect_err("unknown graph direction must fail");
    assert!(err.to_string().contains("unknown variant"), "{err}");

    let bad = serde_json::json!({
        "node_kinds": ["package"],
        "source_ids": [],
        "min_confidence": 0.5,
        "metadata": {},
        "legacy": true
    });

    let err = serde_json::from_value::<GraphQueryFilters>(bad)
        .expect_err("graph filters must reject unknown fields");
    assert!(err.to_string().contains("unknown field"), "{err}");
}

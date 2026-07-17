use super::*;
use axon_api::source::{
    DocumentId, GraphEvidence, GraphNodeCandidate, MetadataMap, SourceId, SourceItemKey,
};

fn node(kind: &str, key: &str, label: &str) -> GraphNodeCandidate {
    GraphNodeCandidate {
        node_kind: kind.to_string(),
        stable_key: key.to_string(),
        label: label.to_string(),
        properties: MetadataMap::new(),
    }
}

fn evidence(kind: &str, confidence: f32) -> GraphEvidence {
    GraphEvidence {
        evidence_id: format!("ev-{kind}"),
        evidence_kind: kind.to_string(),
        source_id: SourceId::new("src"),
        source_item_key: SourceItemKey::new("item"),
        document_id: Some(DocumentId::new("doc")),
        chunk_id: None,
        range: None,
        quote: None,
        confidence,
        metadata: MetadataMap::new(),
    }
}

#[test]
fn node_id_is_deterministic_and_kind_scoped() {
    let a = node_id_for("repo", "https://github.com/x/y");
    let b = node_id_for("repo", "https://github.com/x/y");
    assert_eq!(a, b, "same kind+key must be stable");
    assert!(a.0.starts_with("node_"));

    let c = node_id_for("docs_site", "https://github.com/x/y");
    assert_ne!(a, c, "different kind must yield different node");
}

#[test]
fn edge_id_is_deterministic_over_tuple() {
    let from = node_id_for("repo", "r");
    let to = node_id_for("docs_site", "d");
    let a = edge_id_for("repo_has_docs", &from, &to);
    let b = edge_id_for("repo_has_docs", &from, &to);
    assert_eq!(a, b);
    assert!(a.0.starts_with("edge_"));
    // Reversing endpoints yields a different edge.
    let rev = edge_id_for("repo_has_docs", &to, &from);
    assert_ne!(a, rev);
}

#[test]
fn authority_from_evidence_takes_the_maximum() {
    let ev = vec![
        evidence("text_mention", 0.4),
        evidence("package_repository", 0.9),
    ];
    assert_eq!(authority_from_evidence(&ev), Authority::Official);

    let ev = vec![evidence("text_mention", 0.4)];
    assert_eq!(authority_from_evidence(&ev), Authority::Inferred);

    // Unknown evidence kinds contribute no authority.
    let ev = vec![evidence("mystery_kind", 0.9)];
    assert_eq!(authority_from_evidence(&ev), Authority::Unknown);
}

#[test]
fn confidence_from_evidence_is_max_with_fallback() {
    let ev = vec![evidence("sitemap", 0.3), evidence("redirect", 0.7)];
    assert!((confidence_from_evidence(&ev, 0.1) - 0.7).abs() < 1e-6);
    // Empty evidence falls back.
    assert!((confidence_from_evidence(&[], 0.55) - 0.55).abs() < 1e-6);
}

#[test]
fn resolve_edge_requires_both_endpoints_present() {
    let nodes = vec![
        resolve_node(&node("repo", "r", "repo")),
        resolve_node(&node("docs_site", "d", "docs")),
    ];
    let edge = axon_api::source::GraphEdgeCandidate {
        edge_kind: "repo_has_docs".to_string(),
        from_stable_key: "r".to_string(),
        to_stable_key: "d".to_string(),
        evidence_ids: vec!["ev".to_string()],
        properties: MetadataMap::new(),
    };
    let ev = vec![evidence("github_homepage", 0.9)];
    let resolved = resolve_edge(&edge, &nodes, &ev, 0.5).expect("both endpoints present");
    assert_eq!(resolved.authority, Authority::Official);

    // Dangling endpoint → None.
    let dangling = axon_api::source::GraphEdgeCandidate {
        edge_kind: "repo_has_docs".to_string(),
        from_stable_key: "r".to_string(),
        to_stable_key: "missing".to_string(),
        evidence_ids: vec!["ev".to_string()],
        properties: MetadataMap::new(),
    };
    assert!(resolve_edge(&dangling, &nodes, &ev, 0.5).is_none());
}

#[test]
fn canonical_uri_prefers_property_then_stable_key() {
    let mut n = node("repo", "stable-key-only", "r");
    assert_eq!(canonical_uri_for(&n), "stable-key-only");

    n.properties.0.insert(
        "canonical_uri".to_string(),
        serde_json::Value::String("https://example.com".to_string()),
    );
    assert_eq!(canonical_uri_for(&n), "https://example.com");
}

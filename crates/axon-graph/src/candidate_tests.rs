use super::*;
use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    JobId, MetadataMap, SourceId, SourceItemKey,
};
use uuid::Uuid;

fn base_candidate() -> GraphCandidate {
    GraphCandidate {
        candidate_id: "gc-1".to_string(),
        job_id: JobId::new(Uuid::from_u128(1)),
        source_id: SourceId::new("src"),
        source_item_key: SourceItemKey::new("Cargo.toml"),
        item_canonical_uri: "file:///repo/Cargo.toml".to_string(),
        document_id: None,
        kind: "repo_package".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "local".to_string(),
            parser: Some("manifest".to_string()),
            version: "1".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "repo".to_string(),
                stable_key: "repo:x".to_string(),
                label: "x".to_string(),
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
            edge_kind: "repo_declares_dependency".to_string(),
            from_stable_key: "repo:x".to_string(),
            to_stable_key: "pkg:tokio".to_string(),
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: "ev-1".to_string(),
            evidence_kind: "dependency_manifest".to_string(),
            source_id: SourceId::new("src"),
            source_item_key: SourceItemKey::new("Cargo.toml"),
            document_id: None,
            chunk_id: None,
            range: None,
            quote: None,
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

#[test]
fn valid_candidate_passes() {
    assert!(validate_candidate(&base_candidate()).is_ok());
}

#[test]
fn unknown_node_kind_is_rejected() {
    let mut c = base_candidate();
    c.nodes[0].node_kind = "repository".to_string();
    let err = validate_candidate(&c).unwrap_err();
    assert!(
        err.message.contains("unknown graph node kind"),
        "{}",
        err.message
    );
}

#[test]
fn unknown_edge_kind_is_rejected() {
    let mut c = base_candidate();
    c.edges[0].edge_kind = "depends_on".to_string();
    let err = validate_candidate(&c).unwrap_err();
    assert!(
        err.message.contains("unknown graph edge kind"),
        "{}",
        err.message
    );
}

#[test]
fn candidate_validation_rejects_unknown_evidence_kind() {
    let mut c = base_candidate();
    c.evidence[0].evidence_kind = "tool_result".to_string();
    let err = validate_candidate(&c).unwrap_err();
    assert!(
        err.message.contains("unknown graph evidence kind"),
        "{}",
        err.message
    );
}

#[test]
fn edge_referencing_missing_stable_key_is_rejected() {
    let mut c = base_candidate();
    c.edges[0].to_stable_key = "pkg:not-in-candidate".to_string();
    let err = validate_candidate(&c).unwrap_err();
    assert!(
        err.message.contains("unknown to stable_key"),
        "{}",
        err.message
    );
}

#[test]
fn edges_without_evidence_are_rejected() {
    let mut c = base_candidate();
    c.evidence.clear();
    let err = validate_candidate(&c).unwrap_err();
    assert!(err.message.contains("no evidence"), "{}", err.message);
}

#[test]
fn empty_stable_key_is_rejected() {
    let mut c = base_candidate();
    c.nodes[0].stable_key = "  ".to_string();
    assert!(validate_candidate(&c).is_err());
}

#[test]
fn nodes_only_candidate_without_edges_needs_no_evidence() {
    let mut c = base_candidate();
    c.edges.clear();
    c.evidence.clear();
    assert!(validate_candidate(&c).is_ok());
}

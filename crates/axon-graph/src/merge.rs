//! Candidate → durable node/edge merge logic.
//!
//! Implements the merge strategies and conflict policy from
//! `docs/pipeline-unification/schemas/graph-schema.md`:
//!
//! - `stable_key`: same kind + stable key merge into one node.
//! - `edge_tuple`: same kind/from/to merge evidence onto one edge.
//! - `keep_highest_authority_with_evidence`: authority resolved from evidence,
//!   with equal-authority collisions preserved as explicit conflicts.
//!
//! Node/edge ids are deterministic (UUIDv5 over a fixed namespace) so the same
//! candidate produces the same id on every run — merges are idempotent.

use axon_api::source::{
    GraphEdgeCandidate, GraphEdgeId, GraphEvidence, GraphNodeCandidate, GraphNodeId,
};
use uuid::Uuid;

use crate::authority::Authority;
use crate::evidence::EvidenceKind;

/// Fixed namespace for deterministic graph id generation.
const GRAPH_NAMESPACE: Uuid = Uuid::from_u128(0x6178_6f6e_2d67_7261_7068_2d6e_7331_0001);

/// Deterministic node id for a (kind, stable_key) pair.
///
/// Same kind + stable key always yields the same id, so re-ingesting a
/// candidate upserts the same durable node rather than duplicating it.
pub fn node_id_for(kind: &str, stable_key: &str) -> GraphNodeId {
    let seed = format!("node|{kind}|{stable_key}");
    let uuid = Uuid::new_v5(&GRAPH_NAMESPACE, seed.as_bytes());
    GraphNodeId::new(format!("node_{}", uuid.simple()))
}

/// Deterministic edge id for a (kind, from_node_id, to_node_id) tuple.
pub fn edge_id_for(kind: &str, from: &GraphNodeId, to: &GraphNodeId) -> GraphEdgeId {
    let seed = format!("edge|{kind}|{}|{}", from.0, to.0);
    let uuid = Uuid::new_v5(&GRAPH_NAMESPACE, seed.as_bytes());
    GraphEdgeId::new(format!("edge_{}", uuid.simple()))
}

/// The canonical URI for a node candidate.
///
/// Node candidates carry stable keys, not canonical URIs. When a candidate
/// property supplies `canonical_uri` we use it; otherwise the stable key is the
/// canonical identity.
pub fn canonical_uri_for(node: &GraphNodeCandidate) -> String {
    node.properties
        .0
        .get("canonical_uri")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| node.stable_key.clone())
}

/// Compute the authority a set of evidence confers on a claim.
///
/// The winning authority is the highest authority across all evidence kinds
/// (source-graph.md: official/user-pinned evidence outranks inferred/community).
/// Unrecognized evidence kinds contribute no authority (`Unknown`).
pub fn authority_from_evidence(evidence: &[GraphEvidence]) -> Authority {
    evidence
        .iter()
        .map(|ev| {
            EvidenceKind::from_str_or_unknown(&ev.evidence_kind)
                .map(EvidenceKind::authority)
                .unwrap_or(Authority::Unknown)
        })
        .max()
        .unwrap_or(Authority::Unknown)
}

/// Confidence for a merged claim: the max confidence across its evidence,
/// falling back to the candidate confidence when evidence carries none.
pub fn confidence_from_evidence(evidence: &[GraphEvidence], fallback: f32) -> f32 {
    evidence
        .iter()
        .map(|ev| ev.confidence)
        .fold(None, |acc: Option<f32>, c| {
            Some(acc.map_or(c, |a| a.max(c)))
        })
        .unwrap_or(fallback)
        .clamp(0.0, 1.0)
}

impl EvidenceKind {
    /// Parse an evidence-kind string, returning `None` for unknown kinds.
    ///
    /// Evidence kinds are advisory for authority computation, so an unknown
    /// evidence kind is tolerated (contributes no authority) rather than
    /// rejecting the whole candidate. Node/edge kinds, by contrast, are hard
    /// rejects.
    pub fn from_str_or_unknown(value: &str) -> Option<EvidenceKind> {
        EvidenceKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
}

/// A node candidate resolved to its deterministic durable identity.
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    pub node_id: GraphNodeId,
    pub kind: String,
    pub stable_key: String,
    pub canonical_uri: String,
    pub label: String,
    pub authority: Authority,
    pub properties: axon_api::source::MetadataMap,
}

/// Resolve a node candidate to its durable identity (id + canonical uri).
///
/// The node's baseline authority is `Inferred` — a plain candidate with no
/// authoritative evidence is inferred, never official.
pub fn resolve_node(node: &GraphNodeCandidate) -> ResolvedNode {
    ResolvedNode {
        node_id: node_id_for(&node.node_kind, &node.stable_key),
        kind: node.node_kind.clone(),
        stable_key: node.stable_key.clone(),
        canonical_uri: canonical_uri_for(node),
        label: node.label.clone(),
        authority: Authority::Inferred,
        properties: node.properties.clone(),
    }
}

/// A resolved edge tuple, ready to upsert or merge evidence onto.
#[derive(Debug, Clone)]
pub struct ResolvedEdge {
    pub edge_id: GraphEdgeId,
    pub kind: String,
    pub from_node_id: GraphNodeId,
    pub to_node_id: GraphNodeId,
    pub authority: Authority,
    pub confidence: f32,
    pub properties: axon_api::source::MetadataMap,
}

/// Resolve an edge candidate to its durable tuple identity, resolving both
/// endpoints by their stable keys.
///
/// Returns `None` when either endpoint stable key is not among the candidate's
/// nodes (a dangling edge). Authority and confidence come from `evidence`.
pub fn resolve_edge(
    edge: &GraphEdgeCandidate,
    resolved_nodes: &[ResolvedNode],
    evidence: &[GraphEvidence],
    fallback_confidence: f32,
) -> Option<ResolvedEdge> {
    let from = resolved_nodes
        .iter()
        .find(|n| n.stable_key == edge.from_stable_key)?
        .node_id
        .clone();
    let to = resolved_nodes
        .iter()
        .find(|n| n.stable_key == edge.to_stable_key)?
        .node_id
        .clone();
    Some(ResolvedEdge {
        edge_id: edge_id_for(&edge.edge_kind, &from, &to),
        kind: edge.edge_kind.clone(),
        from_node_id: from,
        to_node_id: to,
        authority: authority_from_evidence(evidence),
        confidence: confidence_from_evidence(evidence, fallback_confidence),
        properties: edge.properties.clone(),
    })
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;

//! Source graph registry used by schema-contract generation.
//!
//! This projects the closed [`crate::node::GraphNodeKind`] and
//! [`crate::edge::GraphEdgeKind`] enums (the canonical registry mirroring
//! `docs/pipeline-unification/sources/source-graph.md`) into the flat shape
//! the `xtask schemas graph` generator consumes. It must never hand-maintain
//! a second kind list — `node_kind_registry()`/`edge_kind_registry()` are
//! derived from `GraphNodeKind::ALL`/`GraphEdgeKind::ALL` so the generated
//! `graph.schema.json`/`graph.md` stay in lockstep with the enums (and, by
//! the enums' own doc contract, with `source-graph.md`).

use axon_api::source::{AuthorityLevel, GraphKindDocument};

use crate::edge::GraphEdgeKind;
use crate::evidence::EvidenceKind;
use crate::node::GraphNodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphKindSpec {
    pub kind: &'static str,
    pub requires_evidence: bool,
}

/// Every closed node kind, in `GraphNodeKind::ALL` registry order.
///
/// All node kinds require evidence before merge into `GraphStore`
/// (`candidate.rs` validates every candidate node against the closed enum
/// and the write path is evidence-gated) — there is currently no node kind
/// that bypasses evidence.
pub fn node_kind_registry() -> Vec<GraphKindSpec> {
    GraphNodeKind::ALL
        .iter()
        .map(|kind| GraphKindSpec {
            kind: kind.as_str(),
            requires_evidence: true,
        })
        .collect()
}

/// Every closed edge kind, in `GraphEdgeKind::ALL` registry order.
///
/// All edge kinds require evidence — the `GraphEdge` schema
/// (graph-schema.md) makes `evidence` a required array field.
pub fn edge_kind_registry() -> Vec<GraphKindSpec> {
    GraphEdgeKind::ALL
        .iter()
        .map(|kind| GraphKindSpec {
            kind: kind.as_str(),
            requires_evidence: true,
        })
        .collect()
}

/// The full closed-registry kind document served by `GET /v1/graph/kinds`
/// (REST) and `graph.kinds` (MCP). Derived directly from the same closed
/// enums `node_kind_registry`/`edge_kind_registry` project, plus
/// [`EvidenceKind::ALL`] and every [`AuthorityLevel`] variant, so it can never
/// drift from the registries that gate candidate validation.
pub fn kind_document() -> GraphKindDocument {
    GraphKindDocument {
        node_kinds: GraphNodeKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        edge_kinds: GraphEdgeKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        evidence_kinds: EvidenceKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect(),
        authority_levels: vec![
            AuthorityLevel::Official,
            AuthorityLevel::Verified,
            AuthorityLevel::UserPinned,
            AuthorityLevel::Inferred,
            AuthorityLevel::Community,
            AuthorityLevel::Mirror,
            AuthorityLevel::Conflicting,
            AuthorityLevel::Unknown,
        ],
    }
}

#[cfg(test)]
#[path = "schema_registry_tests.rs"]
mod tests;

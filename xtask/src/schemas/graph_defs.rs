//! `$defs` builders for the `graph` schema family.
//!
//! Every def here is derived from real source, never hand-fabricated:
//! - `GraphNode`/`GraphEdge`/`GraphEvidence`/`GraphCandidate`/... come from
//!   `schemars::schema_for!` over the actual `axon_api::source` DTOs.
//! - `GraphNodeKind`/`GraphEdgeKind` enums are built from
//!   `axon_graph::schema_registry::{node_kind_registry, edge_kind_registry}`,
//!   which themselves project the closed `GraphNodeKind`/`GraphEdgeKind`
//!   Rust enums (see graph-schema.md "Required Kind Registry").
//! - `GraphMergeRule` is the literal, contract-documented merge-strategy enum
//!   from `docs/pipeline-unification/schemas/graph-schema.md` ("Merge
//!   Rules"), not invented data.
//!
//! `GraphKindRegistry` intentionally stays at the minimal
//! `{kind, type, requires_evidence}` shape documented in
//! `crates/axon-graph/src/schema_registry.rs`. The richer per-kind contract
//! shape in graph-schema.md ("Kind Registry Shape": stable_key_template,
//! required_properties, optional_properties, allowed_evidence_kinds,
//! parser_families, merge, fixtures) has no source-of-truth data anywhere in
//! the codebase yet — authoring 138 per-kind entries here would fabricate
//! registry data instead of generating it. That is tracked as a follow-up
//! that must land in `axon-graph`/`source-graph.md` first.

use serde_json::{Value, json};

use axon_graph::schema_registry::{edge_kind_registry, node_kind_registry};

/// Schemas generated straight from `axon_api::source` DTOs via schemars.
pub(super) fn graph_dto_defs() -> Vec<(&'static str, Value)> {
    vec![
        (
            "GraphNode",
            schemars::schema_for!(axon_api::source::GraphNode).into(),
        ),
        (
            "GraphEdge",
            schemars::schema_for!(axon_api::source::GraphEdge).into(),
        ),
        (
            "GraphEvidence",
            schemars::schema_for!(axon_api::source::GraphEvidence).into(),
        ),
        (
            "GraphCandidate",
            schemars::schema_for!(axon_api::source::GraphCandidate).into(),
        ),
        (
            "GraphResolveRequest",
            schemars::schema_for!(axon_api::source::GraphResolveRequest).into(),
        ),
        (
            "GraphQueryRequest",
            schemars::schema_for!(axon_api::source::GraphQueryRequest).into(),
        ),
        (
            "GraphQueryResult",
            schemars::schema_for!(axon_api::source::GraphQueryResult).into(),
        ),
    ]
}

/// `GraphNodeKind`/`GraphEdgeKind` string enums, derived from the closed
/// registries (never a hand-maintained list).
pub(super) fn graph_kind_enum_defs() -> Vec<(&'static str, Value)> {
    let node_values: Vec<&str> = node_kind_registry().iter().map(|k| k.kind).collect();
    let edge_values: Vec<&str> = edge_kind_registry().iter().map(|k| k.kind).collect();
    vec![
        (
            "GraphNodeKind",
            json!({
                "type": "string",
                "enum": node_values,
                "x-axon": {"rust_enum": "GraphNodeKind", "owner_crate": "axon-graph"}
            }),
        ),
        (
            "GraphEdgeKind",
            json!({
                "type": "string",
                "enum": edge_values,
                "x-axon": {"rust_enum": "GraphEdgeKind", "owner_crate": "axon-graph"}
            }),
        ),
    ]
}

/// Literal, contract-documented merge-strategy enum (graph-schema.md "Merge
/// Rules" table) — not derived from a Rust type because none exists yet.
pub(super) fn graph_merge_rule_def() -> (&'static str, Value) {
    (
        "GraphMergeRule",
        json!({
            "type": "object",
            "required": ["strategy", "conflict_policy"],
            "properties": {
                "strategy": {
                    "type": "string",
                    "enum": ["stable_key", "edge_tuple", "versioned", "never_merge"]
                },
                "conflict_policy": {
                    "type": "string",
                    "enum": [
                        "keep_highest_authority_with_evidence",
                        "keep_all_as_conflict",
                        "last_observed_wins",
                        "manual_review"
                    ]
                },
                "confidence_floor": {"type": "number", "minimum": 0, "maximum": 1}
            },
            "additionalProperties": false
        }),
    )
}

/// Minimal `GraphKindRegistry` item shape: `{kind, type, requires_evidence}`.
/// See the module doc comment for why the richer per-kind shape is not
/// generated here.
pub(super) fn graph_kind_registry_item_def() -> (&'static str, Value) {
    (
        "GraphKindRegistry",
        json!({
            "type": "object",
            "required": ["kind", "type", "requires_evidence"],
            "properties": {
                "kind": {"type": "string"},
                "type": {"type": "string", "enum": ["node", "edge"]},
                "requires_evidence": {"type": "boolean"}
            },
            "additionalProperties": true
        }),
    )
}

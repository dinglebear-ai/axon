//! Candidate validation.
//!
//! Parsers and adapters emit [`GraphCandidate`] values, not durable rows. Before
//! a candidate is written it must validate against the closed kind registry and
//! the candidate rules from `graph-schema.md`:
//!
//! - node candidates refer to stable keys, not generated `node_id`s;
//! - every node/edge kind must parse into the closed registry;
//! - edge candidates refer to node stable keys that exist in the candidate;
//! - every edge candidate references at least one evidence record.

use std::str::FromStr;

use axon_api::source::{ApiError, GraphCandidate, SourceRange};

use crate::edge::GraphEdgeKind;
use crate::error::graph_validation_error;
use crate::evidence::EvidenceKind;
use crate::node::GraphNodeKind;

/// Validate a candidate against the closed kind registry and candidate rules.
///
/// Returns the first violation as a `Validation`-stage [`ApiError`] so the
/// caller can degrade or fail the source item per scope policy. Unknown node or
/// edge kinds are hard rejects ("graph store rejects unknown kinds before
/// write").
pub fn validate_candidate(candidate: &GraphCandidate) -> Result<(), ApiError> {
    // Every node kind must be in the closed registry.
    for node in &candidate.nodes {
        GraphNodeKind::from_str(&node.node_kind)?;
        if node.stable_key.trim().is_empty() {
            return Err(graph_validation_error(format!(
                "node candidate of kind {:?} has an empty stable_key",
                node.node_kind
            )));
        }
    }

    // Every edge kind must be in the closed registry, both endpoints must refer
    // to a node stable key present in the candidate, and the candidate must
    // carry at least one evidence record for its edges.
    for edge in &candidate.edges {
        GraphEdgeKind::from_str(&edge.edge_kind)?;
        if !candidate
            .nodes
            .iter()
            .any(|n| n.stable_key == edge.from_stable_key)
        {
            return Err(graph_validation_error(format!(
                "edge {:?} references unknown from stable_key {:?}",
                edge.edge_kind, edge.from_stable_key
            )));
        }
        if !candidate
            .nodes
            .iter()
            .any(|n| n.stable_key == edge.to_stable_key)
        {
            return Err(graph_validation_error(format!(
                "edge {:?} references unknown to stable_key {:?}",
                edge.edge_kind, edge.to_stable_key
            )));
        }
    }

    // Edges are claims with evidence — if a candidate declares edges it must
    // carry evidence to justify them (source-graph.md: "Edges are never just
    // true").
    if !candidate.edges.is_empty() && candidate.evidence.is_empty() {
        return Err(graph_validation_error(format!(
            "candidate {:?} declares edges but carries no evidence",
            candidate.candidate_id
        )));
    }

    for evidence in &candidate.evidence {
        EvidenceKind::from_str(&evidence.evidence_kind)?;
        if let Some(range) = &evidence.range {
            validate_range_order(range)?;
        }
    }

    Ok(())
}

fn validate_range_order(range: &SourceRange) -> Result<(), ApiError> {
    if starts_after(range.line_start, range.line_end)
        || starts_after(range.byte_start, range.byte_end)
        || starts_after(range.char_start, range.char_end)
        || starts_after(range.time_start_ms, range.time_end_ms)
        || range
            .turn_start
            .as_ref()
            .zip(range.turn_end.as_ref())
            .is_some_and(|(start, end)| start > end)
    {
        return Err(graph_validation_error(
            "invalid source range on graph evidence",
        ));
    }
    Ok(())
}

fn starts_after<T: Ord>(start: Option<T>, end: Option<T>) -> bool {
    start.zip(end).is_some_and(|(start, end)| start > end)
}

#[cfg(test)]
#[path = "candidate_tests.rs"]
mod tests;

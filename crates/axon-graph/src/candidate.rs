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

use std::collections::HashSet;
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
    validate_candidate_identity(candidate)?;
    validate_confidence("candidate", candidate.confidence)?;

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

    // Edges are claims with evidence — if a candidate declares edges it must
    // carry evidence to justify them (source-graph.md: "Edges are never just
    // true"). Check this before resolving individual references so callers get
    // the direct missing-evidence diagnostic.
    if !candidate.edges.is_empty() && candidate.evidence.is_empty() {
        return Err(graph_validation_error(format!(
            "candidate {:?} declares edges but carries no evidence",
            candidate.candidate_id
        )));
    }

    let mut evidence_ids = HashSet::with_capacity(candidate.evidence.len());
    for evidence in &candidate.evidence {
        if !evidence_ids.insert(evidence.evidence_id.as_str()) {
            return Err(graph_validation_error(format!(
                "candidate {:?} contains duplicate evidence id {:?}",
                candidate.candidate_id, evidence.evidence_id
            )));
        }
    }

    // Every edge kind must be in the closed registry, both endpoints must refer
    // to a node stable key present in the candidate, and every edge must name
    // the evidence records that justify that specific claim.
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
        if edge.evidence_ids.is_empty() {
            return Err(graph_validation_error(format!(
                "edge {:?} carries no evidence references",
                edge.edge_kind
            )));
        }
        for evidence_id in &edge.evidence_ids {
            if !evidence_ids.contains(evidence_id.as_str()) {
                return Err(graph_validation_error(format!(
                    "edge {:?} references unknown evidence id {:?}",
                    edge.edge_kind, evidence_id
                )));
            }
        }
    }

    for evidence in &candidate.evidence {
        EvidenceKind::from_str(&evidence.evidence_kind)?;
        validate_confidence("graph evidence", evidence.confidence)?;
        validate_evidence_lineage(candidate, evidence)?;
        if let Some(range) = &evidence.range {
            validate_range_order(range)?;
        }
    }

    Ok(())
}

fn validate_evidence_lineage(
    candidate: &GraphCandidate,
    evidence: &axon_api::source::GraphEvidence,
) -> Result<(), ApiError> {
    if evidence.source_id != candidate.source_id {
        return Err(graph_validation_error(format!(
            "graph evidence {:?} source_id does not match candidate {:?}",
            evidence.evidence_id, candidate.candidate_id
        )));
    }
    if evidence.source_item_key != candidate.source_item_key {
        return Err(graph_validation_error(format!(
            "graph evidence {:?} source_item_key does not match candidate {:?}",
            evidence.evidence_id, candidate.candidate_id
        )));
    }
    if let Some(document_id) = &candidate.document_id
        && evidence.document_id.as_ref() != Some(document_id)
    {
        return Err(graph_validation_error(format!(
            "graph evidence {:?} document_id does not match candidate {:?}",
            evidence.evidence_id, candidate.candidate_id
        )));
    }
    Ok(())
}

fn validate_candidate_identity(candidate: &GraphCandidate) -> Result<(), ApiError> {
    for (field, value) in [
        ("candidate_id", candidate.candidate_id.as_str()),
        ("kind", candidate.kind.as_str()),
        ("item_canonical_uri", candidate.item_canonical_uri.as_str()),
        ("producer.adapter", candidate.producer.adapter.as_str()),
        ("producer.version", candidate.producer.version.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(graph_validation_error(format!(
                "graph candidate {:?} has an empty {field}",
                candidate.candidate_id
            )));
        }
    }

    if let Some(merge_key) = &candidate.merge_key {
        validate_merge_key(candidate, merge_key)?;
    }

    Ok(())
}

fn validate_merge_key(candidate: &GraphCandidate, merge_key: &str) -> Result<(), ApiError> {
    let trimmed = merge_key.trim();
    if trimmed.is_empty() {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} has an empty merge_key",
            candidate.candidate_id
        )));
    }
    if trimmed.len() != merge_key.len() {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key must not contain leading or trailing whitespace",
            candidate.candidate_id
        )));
    }
    if trimmed.len() > 512 {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key exceeds 512 bytes",
            candidate.candidate_id
        )));
    }
    let Some((namespace, value)) = trimmed.split_once(':') else {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key must include a namespace prefix",
            candidate.candidate_id
        )));
    };
    if namespace.is_empty() || value.is_empty() {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key must include non-empty namespace and value",
            candidate.candidate_id
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key contains invalid control characters",
            candidate.candidate_id
        )));
    }
    if is_unstable_merge_namespace(namespace) {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key uses unstable run-scoped namespace {:?}",
            candidate.candidate_id, namespace
        )));
    }
    if trimmed == candidate.candidate_id {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key must not be the candidate_id",
            candidate.candidate_id
        )));
    }
    let job_id = candidate.job_id.0.to_string();
    if trimmed.contains(&job_id) {
        return Err(graph_validation_error(format!(
            "graph candidate {:?} merge_key contains unstable job_id",
            candidate.candidate_id
        )));
    }
    Ok(())
}

fn is_unstable_merge_namespace(namespace: &str) -> bool {
    matches!(
        namespace.to_ascii_lowercase().as_str(),
        "job" | "stage" | "run" | "attempt" | "candidate" | "candidate_id"
    )
}

fn validate_confidence(label: &str, confidence: f32) -> Result<(), ApiError> {
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return Err(graph_validation_error(format!(
            "{label} confidence must be finite and between 0 and 1"
        )));
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

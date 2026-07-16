//! Shared memory-search graph ref enrichment.

use std::collections::BTreeSet;

use axon_api::source::*;
use axon_graph::store::GraphStore;

use crate::store::Result;

pub(crate) async fn graph_refs_for_memory_results(
    graph: Option<&dyn GraphStore>,
    results: &[MemorySearchMatch],
    warnings: &mut Vec<SourceWarning>,
) -> Result<Option<GraphQueryResult>> {
    let Some(graph) = graph else {
        warnings.push(SourceWarning {
            code: "memory.graph_unavailable".to_string(),
            severity: Severity::Degraded,
            message:
                "memory search could not include graph refs because no graph store was configured"
                    .to_string(),
            source_item_key: None,
            retryable: false,
        });
        return Ok(None);
    };
    if results.is_empty() {
        return Ok(Some(GraphQueryResult {
            nodes: Vec::new(),
            edges: Vec::new(),
            evidence: Vec::new(),
            next_cursor: None,
            warnings: Vec::new(),
        }));
    }

    let identifiers = results
        .iter()
        .map(|hit| GraphIdentifier {
            kind: "memory".to_string(),
            canonical_uri: None,
            value: Some(memory_stable_key(&hit.record.memory_id)),
            node_id: None,
            source_id: None,
            source_item_key: None,
            metadata: MetadataMap::new(),
        })
        .collect();
    let resolved = graph
        .resolve(GraphResolveRequest {
            identifiers,
            include_edges: true,
        })
        .await?;
    let mut graph_warnings = resolved.warnings;
    if !resolved.misses.is_empty() {
        graph_warnings.push(SourceWarning {
            code: "memory.graph_missing".to_string(),
            severity: Severity::Info,
            message: format!(
                "{} memory search hit(s) did not have graph mirror refs",
                resolved.misses.len()
            ),
            source_item_key: None,
            retryable: true,
        });
    }

    let mut seen_nodes = BTreeSet::new();
    let mut seen_edges = BTreeSet::new();
    let mut seen_evidence = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    for resolved in resolved.resolved {
        if seen_nodes.insert(resolved.node.node_id.clone()) {
            nodes.push(resolved.node);
        }
        for edge in resolved.edges {
            if seen_edges.insert(edge.edge_id.clone()) {
                for item in &edge.evidence {
                    if seen_evidence.insert(item.evidence_id.clone()) {
                        evidence.push(item.clone());
                    }
                }
                edges.push(edge);
            }
        }
        for item in resolved.evidence {
            if seen_evidence.insert(item.evidence_id.clone()) {
                evidence.push(item);
            }
        }
    }
    warnings.extend(graph_warnings.clone());
    Ok(Some(GraphQueryResult {
        nodes,
        edges,
        evidence,
        next_cursor: None,
        warnings: graph_warnings,
    }))
}

fn memory_stable_key(memory_id: &MemoryId) -> String {
    format!("memory:{}", memory_id.0)
}

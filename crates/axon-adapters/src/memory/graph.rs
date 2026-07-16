use std::collections::BTreeSet;

use axon_api::source::*;

use super::{ADAPTER_NAME, memory_metadata};

pub(super) fn memory_graph_candidates(
    plan: &SourcePlan,
    record: &MemoryRecord,
    item: &AcquiredSourceItem,
) -> Vec<GraphCandidate> {
    let memory_key = format!("memory:{}", record.memory_id.0);
    let mut nodes = vec![GraphNodeCandidate {
        node_kind: "memory".to_string(),
        stable_key: memory_key.clone(),
        label: record
            .title
            .clone()
            .unwrap_or_else(|| record.memory_id.0.clone()),
        properties: memory_metadata(record),
    }];
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::from([memory_key.clone()]);

    for (index, link) in record.links.iter().enumerate() {
        let (node_kind, stable_key, edge_kind) = linked_target(link);
        if seen.insert(stable_key.clone()) {
            nodes.push(GraphNodeCandidate {
                node_kind,
                stable_key: stable_key.clone(),
                label: link.target.clone(),
                properties: MetadataMap::new(),
            });
        }
        edges.push(GraphEdgeCandidate {
            edge_kind,
            from_stable_key: memory_key.clone(),
            to_stable_key: stable_key,
            properties: MetadataMap::new(),
        });
        if link.evidence.is_empty() {
            evidence.push(graph_evidence(plan, record, item, format!("link-{index}")));
        } else {
            evidence.extend(
                link.evidence
                    .iter()
                    .cloned()
                    .map(|stored| rebase_evidence(stored, plan, record, item)),
            );
        }
    }
    if let Some(replacement) = &record.superseded_by {
        let replacement_key = format!("memory:{}", replacement.0);
        push_memory_node(
            &mut nodes,
            &mut seen,
            replacement_key.clone(),
            replacement.0.clone(),
        );
        edges.push(GraphEdgeCandidate {
            edge_kind: "memory_supersedes".to_string(),
            from_stable_key: replacement_key,
            to_stable_key: memory_key.clone(),
            properties: MetadataMap::new(),
        });
        evidence.push(graph_evidence(plan, record, item, "superseded".to_string()));
    }
    if let Some(conflict) = &record.contradicts {
        let conflict_key = format!("memory:{}", conflict.0);
        push_memory_node(
            &mut nodes,
            &mut seen,
            conflict_key.clone(),
            conflict.0.clone(),
        );
        edges.push(GraphEdgeCandidate {
            edge_kind: "memory_contradicts".to_string(),
            from_stable_key: memory_key.clone(),
            to_stable_key: conflict_key,
            properties: MetadataMap::new(),
        });
        evidence.push(graph_evidence(
            plan,
            record,
            item,
            "contradiction".to_string(),
        ));
    }

    vec![GraphCandidate {
        candidate_id: format!("cand_memory_{}", record.memory_id.0),
        job_id: plan.job_id,
        source_id: plan.route.source.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        item_canonical_uri: item.manifest_item.canonical_uri.clone(),
        document_id: Some(DocumentId::new(record.memory_id.0.clone())),
        kind: "memory_lifecycle".to_string(),
        merge_key: Some(format!("memory_record:{}", record.memory_id.0)),
        producer: GraphCandidateProducer {
            adapter: ADAPTER_NAME.to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes,
        edges,
        evidence,
        confidence: record.confidence,
        metadata: memory_metadata(record),
    }]
}

fn rebase_evidence(
    mut evidence: GraphEvidence,
    plan: &SourcePlan,
    record: &MemoryRecord,
    item: &AcquiredSourceItem,
) -> GraphEvidence {
    if evidence.evidence_kind != "derived_source_attribution" {
        evidence.metadata.insert(
            "memory_original_evidence_kind".to_string(),
            serde_json::json!(evidence.evidence_kind),
        );
        evidence.evidence_kind = "derived_source_attribution".to_string();
    }
    evidence.source_id = plan.route.source.source_id.clone();
    evidence.source_item_key = item.manifest_item.source_item_key.clone();
    evidence.document_id = Some(DocumentId::new(record.memory_id.0.clone()));
    evidence
}

fn linked_target(link: &MemoryLink) -> (String, String, String) {
    let target = link.target.trim();
    let memory_id = target.strip_prefix("memory://").unwrap_or(target);
    if memory_id.starts_with("mem_") {
        return (
            "memory".to_string(),
            format!("memory:{memory_id}"),
            "memory_relates_to".to_string(),
        );
    }
    match link.link_type.to_ascii_lowercase().as_str() {
        "file" | "memory_file" => target_tuple("repo_file", target, "memory_about_file"),
        "issue" | "ticket" => target_tuple("issue", target, "memory_about_issue"),
        "pr" | "pull_request" => target_tuple("pull_request", target, "memory_about_issue"),
        "source" | "repo" | "repository" | "memory_repo" => {
            target_tuple("source", target, "memory_about_source")
        }
        _ => target_tuple("external_resource", target, "memory_relates_to"),
    }
}

fn target_tuple(node_kind: &str, stable_key: &str, edge_kind: &str) -> (String, String, String) {
    (
        node_kind.to_string(),
        stable_key.to_string(),
        edge_kind.to_string(),
    )
}

fn push_memory_node(
    nodes: &mut Vec<GraphNodeCandidate>,
    seen: &mut BTreeSet<String>,
    stable_key: String,
    label: String,
) {
    if seen.insert(stable_key.clone()) {
        nodes.push(GraphNodeCandidate {
            node_kind: "memory".to_string(),
            stable_key,
            label,
            properties: MetadataMap::new(),
        });
    }
}

fn graph_evidence(
    plan: &SourcePlan,
    record: &MemoryRecord,
    item: &AcquiredSourceItem,
    suffix: String,
) -> GraphEvidence {
    GraphEvidence {
        evidence_id: format!("ev_memory_{}_{}", record.memory_id.0, suffix),
        evidence_kind: "derived_source_attribution".to_string(),
        source_id: plan.route.source.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        document_id: Some(DocumentId::new(record.memory_id.0.clone())),
        chunk_id: None,
        range: None,
        quote: None,
        confidence: record.confidence,
        metadata: MetadataMap::new(),
    }
}

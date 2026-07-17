use axon_graph::GraphEdgeKind;

use super::*;

pub(super) fn memory_stable_key(memory_id: &MemoryId) -> String {
    format!("memory:{}", memory_id.0)
}

pub(super) fn link_type_to_edge_kind(link_type: &str) -> GraphEdgeKind {
    match link_type.to_ascii_lowercase().as_str() {
        "source" | "repo" | "repository" => GraphEdgeKind::MemoryAboutSource,
        "file" => GraphEdgeKind::MemoryAboutFile,
        "issue" | "pr" | "pull_request" | "ticket" => GraphEdgeKind::MemoryAboutIssue,
        _ => GraphEdgeKind::MemoryRelatesTo,
    }
}

pub(super) fn memory_node(record: &MemoryRecord) -> GraphNodeCandidate {
    let mut properties = MetadataMap::new();
    properties.insert(
        "memory_status".to_string(),
        serde_json::Value::String(format!("{:?}", record.status).to_lowercase()),
    );
    properties.insert(
        "memory_type".to_string(),
        serde_json::Value::String(format!("{:?}", record.memory_type).to_lowercase()),
    );
    GraphNodeCandidate {
        node_kind: MEMORY_NODE_KIND.to_string(),
        stable_key: memory_stable_key(&record.memory_id),
        label: record
            .title
            .clone()
            .unwrap_or_else(|| record.memory_id.0.clone()),
        properties,
    }
}

pub(super) fn node_only_candidate(
    candidate_id: String,
    node: GraphNodeCandidate,
) -> GraphCandidate {
    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new(MEMORY_SOURCE_ID),
        source_item_key: SourceItemKey::new(candidate_id),
        item_canonical_uri: format!("memory://{}", node.stable_key),
        document_id: None,
        kind: "memory_lifecycle".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: MEMORY_SOURCE_ID.to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes: vec![node],
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 1.0,
        metadata: MetadataMap::new(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn edge_candidate(
    candidate_id: String,
    nodes: Vec<GraphNodeCandidate>,
    edge_kind: GraphEdgeKind,
    from_stable_key: &str,
    to_stable_key: &str,
    reason: Option<&str>,
) -> GraphCandidate {
    let evidence_id = format!("{candidate_id}:evidence");
    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new(MEMORY_SOURCE_ID),
        source_item_key: SourceItemKey::new(candidate_id.clone()),
        item_canonical_uri: format!("memory://{from_stable_key}"),
        document_id: None,
        kind: "memory_lifecycle".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: MEMORY_SOURCE_ID.to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes,
        edges: vec![GraphEdgeCandidate {
            edge_kind: edge_kind.as_str().to_string(),
            from_stable_key: from_stable_key.to_string(),
            to_stable_key: to_stable_key.to_string(),
            evidence_ids: vec![evidence_id.clone()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: MEMORY_EVIDENCE_KIND.to_string(),
            source_id: SourceId::new(MEMORY_SOURCE_ID),
            source_item_key: SourceItemKey::new(candidate_id),
            document_id: None,
            chunk_id: None,
            range: None,
            quote: reason.map(ToOwned::to_owned),
            confidence: 1.0,
            metadata: MetadataMap::new(),
        }],
        confidence: 1.0,
        metadata: MetadataMap::new(),
    }
}

//! Graph evidence projection for structured skills.sh catalog entries.

use axon_api::source::*;
use axon_parse::vertical::VERTICAL_GRAPH_CANDIDATES_METADATA_KEY;

use super::super::SkillsShSkill;
use super::{canonical_source_pointer, digest_id};

pub(super) fn attach_catalog_graph_candidates(
    metadata: &mut MetadataMap,
    plan: &SourcePlan,
    source_id: &SourceId,
    item: &ManifestItem,
    document_id: &DocumentId,
    skill: &SkillsShSkill,
) {
    let graph_candidates = catalog_graph_candidates(plan, source_id, item, document_id, skill);
    if let Ok(value) = serde_json::to_value(graph_candidates) {
        metadata.insert(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(), value);
    }
}

fn catalog_graph_candidates(
    plan: &SourcePlan,
    source_id: &SourceId,
    item: &ManifestItem,
    document_id: &DocumentId,
    skill: &SkillsShSkill,
) -> Vec<GraphCandidate> {
    let (canonical_source_uri, repository, _) = canonical_source_pointer(skill);
    let source_key = format!("source:{}:{}", source_id.0, plan.route.source.canonical_uri);
    let artifact_key = item.source_item_key.0.clone();
    let evidence_id = format!("ev_skills_sh_catalog_{}", digest_id(&skill.id));
    let mut nodes = vec![
        GraphNodeCandidate {
            node_kind: "source".to_string(),
            stable_key: source_key.clone(),
            label: "skills.sh".to_string(),
            properties: canonical_uri_properties(&plan.route.source.canonical_uri),
        },
        GraphNodeCandidate {
            node_kind: "artifact".to_string(),
            stable_key: artifact_key.clone(),
            label: skill.name.clone(),
            properties: canonical_uri_properties(&item.canonical_uri),
        },
    ];
    let mut edges = vec![GraphEdgeCandidate {
        edge_kind: "source_indexed_as".to_string(),
        from_stable_key: source_key,
        to_stable_key: artifact_key.clone(),
        evidence_ids: vec![evidence_id.clone()],
        properties: evidence_only_metadata(),
    }];
    if let Some(repository) = repository {
        let repo_key = format!("repo:github.com/{repository}");
        nodes.push(GraphNodeCandidate {
            node_kind: "repo".to_string(),
            stable_key: repo_key.clone(),
            label: repository,
            properties: canonical_uri_properties(&canonical_source_uri),
        });
        edges.push(GraphEdgeCandidate {
            edge_kind: "derived_from".to_string(),
            from_stable_key: artifact_key,
            to_stable_key: repo_key,
            evidence_ids: vec![evidence_id.clone()],
            properties: evidence_only_metadata(),
        });
    }
    vec![GraphCandidate {
        candidate_id: format!("cand_skills_sh_graph_{}", digest_id(&skill.id)),
        job_id: plan.job_id,
        source_id: source_id.clone(),
        source_item_key: item.source_item_key.clone(),
        item_canonical_uri: item.canonical_uri.clone(),
        document_id: Some(document_id.clone()),
        kind: "artifact_catalog_metadata".to_string(),
        merge_key: Some(format!("skills_sh:{}", item.source_item_key.0)),
        producer: GraphCandidateProducer {
            adapter: "axon-adapters::registry::skills_sh".to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes,
        edges,
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: "text_mention".to_string(),
            source_id: source_id.clone(),
            source_item_key: item.source_item_key.clone(),
            document_id: Some(document_id.clone()),
            chunk_id: None,
            range: None,
            quote: Some(canonical_source_uri),
            confidence: 0.9,
            metadata: evidence_only_metadata(),
        }],
        confidence: 0.9,
        metadata: evidence_only_metadata(),
    }]
}

fn canonical_uri_properties(canonical_uri: &str) -> MetadataMap {
    let mut metadata = evidence_only_metadata();
    metadata.insert(
        "canonical_uri".to_string(),
        serde_json::json!(canonical_uri),
    );
    metadata
}

fn evidence_only_metadata() -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "authorityScope".to_string(),
        serde_json::json!("evidence-only"),
    );
    metadata
}

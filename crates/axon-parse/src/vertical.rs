//! Parser-facing facts derived from vertical extractor metadata.
//!
//! `axon-extract` may still fetch and normalize site-specific documents, but
//! parser facts and graph candidates belong here so the web acquisition bridge
//! can treat vertical metadata exactly like any other parser-produced fact.

use axon_api::source::{
    DocumentId, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence,
    GraphNodeCandidate, JobId, MetadataMap, SourceId, SourceItemKey, SourceParseFacts,
};

pub const VERTICAL_PARSE_FACTS_METADATA_KEY: &str = "_axon_vertical_parse_facts";
pub const VERTICAL_GRAPH_CANDIDATES_METADATA_KEY: &str = "_axon_vertical_graph_candidates";

#[derive(Debug, Clone)]
pub struct VerticalParseInput<'a> {
    pub url: &'a str,
    pub title: Option<&'a str>,
    pub extractor_name: &'a str,
    pub extractor_version: u32,
    pub job_id: JobId,
    pub source_id: &'a SourceId,
    pub document_id: &'a DocumentId,
    pub source_item_key: &'a SourceItemKey,
}

#[derive(Debug, Clone, Default)]
pub struct VerticalParseArtifacts {
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
}

pub fn parse_artifacts(input: VerticalParseInput<'_>) -> VerticalParseArtifacts {
    let mut artifacts = VerticalParseArtifacts::default();
    if input.extractor_name == "github_repo"
        && let Some((owner, repo)) = github_repo_parts(input.url)
    {
        let full_name = format!("{owner}/{repo}");
        artifacts.facts.push(SourceParseFacts {
            document_id: input.document_id.clone(),
            source_item_key: input.source_item_key.clone(),
            fact_kind: "repository".to_string(),
            name: full_name.clone(),
            value: serde_json::json!({
                "git_provider": "github",
                "git_owner": owner,
                "git_repo": repo,
                "vertical": input.extractor_name,
            }),
            parser_id: "vertical_github_repo".to_string(),
            parser_version: input.extractor_version.to_string(),
            parser_method: "vertical_metadata".to_string(),
            range: None,
            confidence: 0.95,
            metadata: MetadataMap::new(),
        });
        artifacts
            .graph_candidates
            .push(github_repo_candidate(input, &owner, &repo));
    }
    artifacts
}

pub fn take_metadata_artifacts(metadata: &mut MetadataMap) -> VerticalParseArtifacts {
    let facts = metadata
        .remove(VERTICAL_PARSE_FACTS_METADATA_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let graph_candidates = metadata
        .remove(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    VerticalParseArtifacts {
        facts,
        graph_candidates,
    }
}

fn github_repo_parts(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.host_str()? != "github.com" {
        return None;
    }
    let mut segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty());
    let owner = segments.next()?.to_string();
    let repo = segments.next()?.to_string();
    segments.next().is_none().then_some((owner, repo))
}

fn github_repo_candidate(input: VerticalParseInput<'_>, owner: &str, repo: &str) -> GraphCandidate {
    let repo_key = format!("repo:github.com/{owner}/{repo}");
    let evidence_id = format!("ev_vertical_github_repo_{owner}_{repo}");
    GraphCandidate {
        candidate_id: format!("cand_vertical_github_repo_{owner}_{repo}"),
        job_id: input.job_id,
        source_id: input.source_id.clone(),
        source_item_key: input.source_item_key.clone(),
        item_canonical_uri: input.url.to_string(),
        document_id: Some(input.document_id.clone()),
        kind: "github_repo_metadata".to_string(),
        merge_key: Some(format!("github_repo:github.com/{owner}/{repo}")),
        producer: GraphCandidateProducer {
            adapter: "axon-adapters::web::vertical".to_string(),
            parser: Some("vertical_github_repo".to_string()),
            version: input.extractor_version.to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "web_page".to_string(),
                stable_key: input.url.to_string(),
                label: input
                    .title
                    .filter(|title| !title.is_empty())
                    .unwrap_or(input.url)
                    .to_string(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "repo".to_string(),
                stable_key: repo_key.clone(),
                label: format!("{owner}/{repo}"),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "official_for".to_string(),
            from_stable_key: input.url.to_string(),
            to_stable_key: repo_key,
            evidence_ids: vec![evidence_id.clone()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: "github_homepage".to_string(),
            source_id: input.source_id.clone(),
            source_item_key: input.source_item_key.clone(),
            document_id: Some(input.document_id.clone()),
            chunk_id: None,
            range: None,
            quote: Some(input.url.to_string()),
            confidence: 0.95,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.95,
        metadata: MetadataMap::new(),
    }
}

#[cfg(test)]
#[path = "vertical_tests.rs"]
mod tests;

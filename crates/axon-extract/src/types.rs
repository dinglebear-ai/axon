//! Shared types returned by vertical extractors.

use axon_api::source::{
    DocumentId, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence,
    GraphNodeCandidate, JobId, MetadataMap, SourceId, SourceItemKey, SourceParseFacts,
};

/// Output of a successful vertical extraction.
///
/// Carries enough information to build a `PreparedDoc` for embedding.
/// The `extractor_name` + `extractor_version` fields flow through to the
/// Qdrant payload so retrieval can filter by source extractor.
#[derive(Debug, Clone)]
pub struct ScrapedDoc {
    pub url: String,
    pub markdown: String,
    pub title: Option<String>,
    /// Stable extractor identifier (e.g. `"github_repo"`, `"pypi"`).
    pub extractor_name: &'static str,
    /// Monotone version bump when extraction logic changes in a
    /// backward-incompatible way (triggers reindex on upgrade).
    pub extractor_version: u32,
    /// Optional structured-data blob (JSON-LD, API response fragment).
    pub structured: Option<serde_json::Value>,
    /// URLs the caller should crawl after embedding this doc (e.g. the docs
    /// site for a crate). Empty for most verticals. Propagated to `ScrapeResult`.
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata fields to merge flat into the Qdrant payload.
    /// Every key becomes a top-level payload field when embedded.
    /// Keys must follow the prefix convention: `pkg_*`, `git_*`, `hf_*`, etc.
    /// Absent beats null — only set keys that have actual values.
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct VerticalParseArtifacts {
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
}

impl ScrapedDoc {
    pub fn parse_artifacts(
        &self,
        job_id: JobId,
        source_id: SourceId,
        document_id: DocumentId,
        source_item_key: SourceItemKey,
    ) -> VerticalParseArtifacts {
        let mut artifacts = VerticalParseArtifacts::default();
        if self.extractor_name == "github_repo"
            && let Some((owner, repo)) = github_repo_parts(&self.url)
        {
            let full_name = format!("{owner}/{repo}");
            artifacts.facts.push(SourceParseFacts {
                document_id: document_id.clone(),
                source_item_key: source_item_key.clone(),
                fact_kind: "repository".to_string(),
                name: full_name.clone(),
                value: serde_json::json!({
                    "git_provider": "github",
                    "git_owner": owner,
                    "git_repo": repo,
                    "vertical": self.extractor_name,
                }),
                parser_id: "vertical_github_repo".to_string(),
                parser_version: self.extractor_version.to_string(),
                parser_method: "vertical_metadata".to_string(),
                range: None,
                confidence: 0.95,
                metadata: MetadataMap::new(),
            });
            artifacts.graph_candidates.push(github_repo_candidate(
                self,
                job_id,
                source_id,
                document_id,
                source_item_key,
                &owner,
                &repo,
            ));
        }
        artifacts
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

fn github_repo_candidate(
    doc: &ScrapedDoc,
    job_id: JobId,
    source_id: SourceId,
    document_id: DocumentId,
    source_item_key: SourceItemKey,
    owner: &str,
    repo: &str,
) -> GraphCandidate {
    let repo_key = format!("repo:github.com/{owner}/{repo}");
    GraphCandidate {
        candidate_id: format!("cand_vertical_github_repo_{owner}_{repo}"),
        job_id,
        source_id: source_id.clone(),
        source_item_key: source_item_key.clone(),
        item_canonical_uri: doc.url.clone(),
        document_id: Some(document_id.clone()),
        kind: "github_repo_metadata".to_string(),
        merge_key: Some(format!("github_repo:github.com/{owner}/{repo}")),
        producer: GraphCandidateProducer {
            adapter: "axon-extract".to_string(),
            parser: Some("vertical_github_repo".to_string()),
            version: doc.extractor_version.to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "web_page".to_string(),
                stable_key: doc.url.clone(),
                label: doc.title.clone().unwrap_or_else(|| doc.url.clone()),
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
            from_stable_key: doc.url.clone(),
            to_stable_key: repo_key,
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!("ev_vertical_github_repo_{owner}_{repo}"),
            evidence_kind: "github_homepage".to_string(),
            source_id,
            source_item_key,
            document_id: Some(document_id),
            chunk_id: None,
            range: None,
            quote: Some(doc.url.clone()),
            confidence: 0.95,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.95,
        metadata: MetadataMap::new(),
    }
}

/// Catalog entry for one registered vertical extractor.
#[derive(Debug, Clone)]
pub struct ExtractorInfo {
    /// Stable machine-readable name — used as the `dispatch_by_name` key.
    pub name: &'static str,
    /// Human-readable label for `axon scrape --list-verticals`.
    pub label: &'static str,
    /// One-sentence description.
    pub description: &'static str,
    /// URL patterns this extractor claims (for documentation / discovery).
    pub url_patterns: &'static [&'static str],
    /// Whether this extractor fires in automatic URL-based dispatch
    /// (`dispatch_by_url`). Set to `false` for antibot-gated or ToS-risky
    /// extractors that require explicit opt-in via `--vertical <name>`.
    pub auto_dispatch: bool,
}

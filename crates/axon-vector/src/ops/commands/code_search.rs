use crate::ops::commands::query::{QueryHitOptions, query_hits_with_options};
use crate::ops::commands::retrieval::CandidateScorePolicy;
use crate::ops::qdrant::build_local_project_code_filter;
use axon_api::QueryHit;
use axon_core::config::Config;

pub struct CodeSearchVectorRequest<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub offset: usize,
    pub project_key: &'a str,
    pub generation: i64,
    pub path_prefix: Option<&'a str>,
}

pub async fn code_search_hits(
    cfg: &Config,
    req: CodeSearchVectorRequest<'_>,
) -> Result<Vec<QueryHit>, Box<dyn std::error::Error + Send + Sync>> {
    query_hits_with_options(
        cfg,
        req.query,
        req.limit,
        req.offset,
        QueryHitOptions {
            command: "code_search",
            filter: Some(build_local_project_code_filter(
                req.project_key,
                req.generation,
                req.path_prefix,
            )),
            allow_short_content: true,
            score_policy: code_search_score_policy(),
        },
    )
    .await
}

pub(crate) fn code_search_score_policy() -> CandidateScorePolicy<'static> {
    CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.0,
        apply_code_search_adjustment: true,
        force_code_intent: true,
        min_relevance_score: None,
        require_topical_overlap: false,
    }
}

#[cfg(test)]
#[path = "code_search_tests.rs"]
mod tests;

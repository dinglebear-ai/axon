use crate::core::ask_explain::AskExplainScoreKind;
use crate::vector::ops::commands::retrieval::{
    CandidateRankingTrace, CandidateScorePolicy, RetrievedCandidate, score_and_filter_candidates,
    score_and_filter_candidates_with_trace, score_rrf_candidates_with_trace,
};
use crate::vector::ops::tei::qdrant_store::VectorMode;

pub(in crate::vector::ops::commands::ask::context) struct RerankParams<'a> {
    pub(in crate::vector::ops::commands::ask::context) authoritative_domains: &'a [String],
    pub(in crate::vector::ops::commands::ask::context) authoritative_boost: f64,
    pub(in crate::vector::ops::commands::ask::context) product_authority_boost: f64,
    pub(in crate::vector::ops::commands::ask::context) min_relevance_score: f64,
}

#[cfg(test)]
pub(in crate::vector::ops::commands::ask::context) fn is_rrf_mode(
    vector_mode: VectorMode,
    hybrid_search_enabled: bool,
    sparse_was_empty: bool,
) -> bool {
    matches!(vector_mode, VectorMode::Named) && hybrid_search_enabled && !sparse_was_empty
}

pub(in crate::vector::ops::commands::ask::context) fn apply_mode_aware_rerank(
    is_rrf: bool,
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    params: &RerankParams<'_>,
) -> Vec<RetrievedCandidate> {
    let score_policy = CandidateScorePolicy {
        authoritative_domains: params.authoritative_domains,
        authoritative_boost: params.authoritative_boost,
        product_authority_boost: params.product_authority_boost,
        apply_code_search_adjustment: false,
        force_code_intent: false,
        min_relevance_score: if is_rrf {
            None
        } else {
            Some(params.min_relevance_score)
        },
        require_topical_overlap: true,
    };
    score_and_filter_candidates(candidates, query_tokens, &score_policy)
}

pub(in crate::vector::ops::commands::ask::context) fn apply_mode_aware_rerank_with_trace(
    is_rrf: bool,
    dense_score_kind: AskExplainScoreKind,
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    params: &RerankParams<'_>,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    if is_rrf {
        let score_policy = CandidateScorePolicy {
            authoritative_domains: params.authoritative_domains,
            authoritative_boost: params.authoritative_boost,
            product_authority_boost: params.product_authority_boost,
            apply_code_search_adjustment: false,
            force_code_intent: false,
            min_relevance_score: None,
            require_topical_overlap: true,
        };
        return score_rrf_candidates_with_trace(candidates, query_tokens, &score_policy);
    }

    let score_policy = CandidateScorePolicy {
        authoritative_domains: params.authoritative_domains,
        authoritative_boost: params.authoritative_boost,
        product_authority_boost: params.product_authority_boost,
        apply_code_search_adjustment: false,
        force_code_intent: false,
        min_relevance_score: Some(params.min_relevance_score),
        require_topical_overlap: true,
    };
    score_and_filter_candidates_with_trace(
        candidates,
        query_tokens,
        &score_policy,
        dense_score_kind,
    )
}

pub(in crate::vector::ops::commands::ask::context) fn rerank_with_optional_trace(
    ask_explain: bool,
    rrf_mode: bool,
    dense_score_kind: AskExplainScoreKind,
    retrieved_candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    rerank_params: &RerankParams<'_>,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    if ask_explain {
        apply_mode_aware_rerank_with_trace(
            rrf_mode,
            dense_score_kind,
            retrieved_candidates,
            query_tokens,
            rerank_params,
        )
    } else {
        (
            apply_mode_aware_rerank(rrf_mode, retrieved_candidates, query_tokens, rerank_params),
            Vec::new(),
        )
    }
}

pub(in crate::vector::ops::commands::ask::context) fn retrieval_score_kind(
    vector_mode: VectorMode,
    rrf_mode: bool,
) -> AskExplainScoreKind {
    if rrf_mode {
        AskExplainScoreKind::Rrf
    } else if matches!(vector_mode, VectorMode::Named) {
        AskExplainScoreKind::NamedDense
    } else {
        AskExplainScoreKind::Cosine
    }
}

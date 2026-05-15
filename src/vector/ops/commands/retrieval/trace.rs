use super::{
    CandidateScorePolicy, RetrievedCandidate, candidate_has_topical_overlap,
    product_authority_boost_for_url,
};
use crate::services::types::{
    AskExplainFilterDecision, AskExplainFilterDecisionKind, AskExplainScoreComponent,
    AskExplainScoreComponentStatus, AskExplainScoreKind,
};
use crate::vector::ops::ranking;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct CandidateRankingTrace {
    pub(crate) candidate: RetrievedCandidate,
    pub(crate) score_kind: AskExplainScoreKind,
    pub(crate) score_components: Vec<AskExplainScoreComponent>,
    pub(crate) filter_decisions: Vec<AskExplainFilterDecision>,
}

pub(crate) fn score_and_filter_candidates(
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    policy: &CandidateScorePolicy<'_>,
) -> Vec<RetrievedCandidate> {
    score_and_filter_candidates_inner(
        candidates,
        query_tokens,
        policy,
        false,
        AskExplainScoreKind::Cosine,
    )
    .0
}

#[allow(dead_code)]
pub(crate) fn score_and_filter_candidates_with_trace(
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    policy: &CandidateScorePolicy<'_>,
    score_kind: AskExplainScoreKind,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    let (selected, trace) =
        score_and_filter_candidates_inner(candidates, query_tokens, policy, true, score_kind);
    (selected, trace.unwrap_or_default())
}

pub(crate) fn dropped_candidate_trace(
    candidate: RetrievedCandidate,
    score_kind: AskExplainScoreKind,
    decision_kind: AskExplainFilterDecisionKind,
    reason: &'static str,
) -> CandidateRankingTrace {
    let retrieval_score = candidate.candidate.score;
    CandidateRankingTrace {
        candidate,
        score_kind,
        score_components: vec![score_component(
            "retrieval_score",
            retrieval_score,
            AskExplainScoreComponentStatus::Applied,
            None,
        )],
        filter_decisions: vec![filter_decision(decision_kind, Some(reason))],
    }
}

#[allow(dead_code)]
pub(crate) fn score_rrf_candidates_with_trace(
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    policy: &CandidateScorePolicy<'_>,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    let (selected, trace) = score_and_filter_candidates_inner(
        candidates,
        query_tokens,
        policy,
        true,
        AskExplainScoreKind::Rrf,
    );
    (selected, trace.unwrap_or_default())
}

fn score_and_filter_candidates_inner(
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    policy: &CandidateScorePolicy<'_>,
    trace_enabled: bool,
    score_kind: AskExplainScoreKind,
) -> (Vec<RetrievedCandidate>, Option<Vec<CandidateRankingTrace>>) {
    let raw_candidates = candidates
        .iter()
        .map(|candidate| &candidate.candidate)
        .collect::<Vec<_>>();
    let scored = ranking::score_ask_candidate_ref_breakdowns(
        &raw_candidates,
        query_tokens,
        policy.authoritative_domains,
        policy.authoritative_boost,
    );

    let mut selected = Vec::new();
    let mut traces = trace_enabled.then(|| Vec::with_capacity(candidates.len()));
    for (idx, breakdown) in scored {
        let product_boost = product_authority_boost_for_url(
            &candidates[idx].candidate.url,
            query_tokens,
            policy.product_authority_boost,
        );
        let mut candidate = candidates[idx].clone();
        candidate.candidate.rerank_score = breakdown.rerank_score + product_boost;
        let mut filter_decisions = Vec::new();
        if let Some(min_score) = policy.min_relevance_score
            && breakdown.rerank_score < min_score
        {
            filter_decisions.push(filter_decision(
                AskExplainFilterDecisionKind::DroppedMinRelevance,
                Some("candidate rerank score was below ask_min_relevance_score before product authority boost"),
            ));
        }
        if policy.require_topical_overlap
            && !candidate_has_topical_overlap(raw_candidates[idx], query_tokens)
        {
            filter_decisions.push(filter_decision(
                AskExplainFilterDecisionKind::DroppedTopicalOverlap,
                Some("candidate did not overlap query tokens in URL or chunk text"),
            ));
        }
        let kept = filter_decisions.is_empty();
        if kept {
            filter_decisions.push(filter_decision(AskExplainFilterDecisionKind::Kept, None));
            selected.push(candidate.clone());
        }
        if let Some(traces) = traces.as_mut() {
            traces.push(CandidateRankingTrace {
                candidate,
                score_kind,
                score_components: dense_score_components(&breakdown, product_boost),
                filter_decisions,
            });
        }
    }
    selected.sort_by(|a, b| {
        b.candidate
            .rerank_score
            .partial_cmp(&a.candidate.rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (selected, traces)
}

fn dense_score_components(
    breakdown: &ranking::AskScoreBreakdown,
    product_authority_boost: f64,
) -> Vec<AskExplainScoreComponent> {
    let product_status = if product_authority_boost > 0.0 {
        AskExplainScoreComponentStatus::Applied
    } else {
        AskExplainScoreComponentStatus::NotApplicable
    };
    vec![
        score_component(
            "retrieval_score",
            breakdown.retrieval_score,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
        score_component(
            "lexical_url_token_boost",
            breakdown.lexical_url_token_boost,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
        score_component(
            "lexical_chunk_token_boost",
            breakdown.lexical_chunk_token_boost,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
        score_component(
            "docs_path_boost",
            breakdown.docs_path_boost,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
        score_component(
            "authority_boost",
            breakdown.authority_boost,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
        score_component(
            "product_authority_boost",
            product_authority_boost,
            product_status,
            None,
        ),
        score_component(
            "phrase_match_boost",
            breakdown.phrase_match_boost,
            AskExplainScoreComponentStatus::Applied,
            None,
        ),
    ]
}

fn score_component(
    name: &str,
    value: f64,
    status: AskExplainScoreComponentStatus,
    reason: Option<&str>,
) -> AskExplainScoreComponent {
    AskExplainScoreComponent {
        name: name.to_string(),
        value,
        status,
        reason: reason.map(str::to_string),
    }
}

fn filter_decision(
    kind: AskExplainFilterDecisionKind,
    reason: Option<&str>,
) -> AskExplainFilterDecision {
    AskExplainFilterDecision {
        kind,
        reason: reason.map(str::to_string),
    }
}

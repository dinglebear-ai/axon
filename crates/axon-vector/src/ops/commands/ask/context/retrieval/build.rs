use super::SearchHitsResult;
use crate::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateRankingTrace, RetrievedCandidate, build_candidates_from_hits,
    build_candidates_from_hits_with_trace, merge_candidates, merge_candidates_with_trace,
};
use crate::ops::qdrant;
use axon_core::ask_explain::AskExplainScoreKind;
use axon_core::logging::log_warn;

pub(super) struct AskCandidateBuild {
    pub(super) retrieved_candidates: Vec<RetrievedCandidate>,
    pub(super) pre_rerank_traces: Vec<CandidateRankingTrace>,
    pub(super) warnings: Vec<String>,
}

pub(super) fn build_ask_candidates(
    hits: Vec<qdrant::QdrantSearchHit>,
    secondary_res: Option<SearchHitsResult>,
    build_policy: &CandidateBuildPolicy,
    trace_score_kind: Option<AskExplainScoreKind>,
) -> AskCandidateBuild {
    let mut pre_rerank_traces = Vec::new();
    let mut warnings = Vec::new();
    let mut retrieved_candidates =
        build_primary_candidates(hits, build_policy, trace_score_kind, &mut pre_rerank_traces);

    if let Some(secondary_res) = secondary_res {
        match secondary_res {
            Ok(kw_hits) => {
                let secondary = build_primary_candidates(
                    kw_hits,
                    build_policy,
                    trace_score_kind,
                    &mut pre_rerank_traces,
                );
                retrieved_candidates = merge_candidate_sets(
                    retrieved_candidates,
                    secondary,
                    trace_score_kind,
                    &mut pre_rerank_traces,
                );
            }
            Err(e) => {
                log_warn(&format!(
                    "ask: keyword search failed, continuing with NL only: {e}"
                ));
                let warning = keyword_search_warning();
                warnings.push(warning);
            }
        }
    }
    AskCandidateBuild {
        retrieved_candidates,
        pre_rerank_traces,
        warnings,
    }
}

fn keyword_search_warning() -> String {
    "ask: keyword search failed; continuing with natural-language retrieval only".to_string()
}

fn build_primary_candidates(
    hits: Vec<qdrant::QdrantSearchHit>,
    build_policy: &CandidateBuildPolicy,
    trace_score_kind: Option<AskExplainScoreKind>,
    pre_rerank_traces: &mut Vec<CandidateRankingTrace>,
) -> Vec<RetrievedCandidate> {
    if let Some(score_kind) = trace_score_kind {
        let built = build_candidates_from_hits_with_trace(hits, build_policy, score_kind);
        pre_rerank_traces.extend(built.filter_traces);
        built.candidates
    } else {
        build_candidates_from_hits(hits, build_policy)
    }
}

fn merge_candidate_sets(
    primary: Vec<RetrievedCandidate>,
    secondary: Vec<RetrievedCandidate>,
    trace_score_kind: Option<AskExplainScoreKind>,
    pre_rerank_traces: &mut Vec<CandidateRankingTrace>,
) -> Vec<RetrievedCandidate> {
    if let Some(score_kind) = trace_score_kind {
        let merged = merge_candidates_with_trace(primary, secondary, score_kind);
        pre_rerank_traces.extend(merged.filter_traces);
        merged.candidates
    } else {
        merge_candidates(primary, secondary)
    }
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;

//! Explain trace for the ask-specific post-retrieval ranking path.

use axon_core::ask_explain::{
    AskExplainCandidate, AskExplainContext, AskExplainContextSource, AskExplainFilterDecisionKind,
    AskExplainFullDocFetchMode, AskExplainFullDocFetchSkipReason, AskExplainInsertionMode,
    AskExplainMode, AskExplainRetrieval, AskExplainScoreKind, AskExplainSelectionDecision,
    AskExplainSelectionDecisionKind, AskExplainTrace,
};
use axon_core::config::Config;

use super::ranking::{RankedCandidate, RankingResult};

const CANDIDATE_TRACE_LIMIT: usize = 50;
const SNIPPET_MAX_CHARS: usize = 280;

pub(crate) fn build_explain_trace(
    cfg: &Config,
    question: &str,
    ranked: &RankingResult,
    context: &str,
) -> AskExplainTrace {
    let score_kind = if cfg.hybrid_search_enabled {
        AskExplainScoreKind::Rrf
    } else {
        AskExplainScoreKind::NamedDense
    };
    let mut trace_order = ranked.ranked_indices.clone();
    trace_order.extend(
        ranked
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (!candidate.kept()).then_some(index)),
    );
    let truncated = trace_order.len() > CANDIDATE_TRACE_LIMIT;
    let candidates = trace_order
        .into_iter()
        .take(CANDIDATE_TRACE_LIMIT)
        .map(|index| candidate_trace(&ranked.candidates[index], score_kind))
        .collect();

    let mut selected = ranked
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .selected_context_rank
                .map(|rank| (rank, candidate))
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|(rank, _)| *rank);

    AskExplainTrace {
        mode: AskExplainMode::ExplainOnly,
        retrieval: AskExplainRetrieval {
            query: question.to_string(),
            keyword_query: ranked.keyword_query.clone(),
            dual_search: false,
            collection: cfg.collection.clone(),
            candidate_limit: cfg.ask_candidate_limit,
            hybrid_search_enabled: cfg.hybrid_search_enabled,
            hybrid_candidate_limit: cfg.ask_hybrid_candidates,
            score_kind,
            vector_mode: if cfg.hybrid_search_enabled {
                "named_hybrid_rrf".to_string()
            } else {
                "named_dense".to_string()
            },
            sparse_query_status: None,
        },
        candidates,
        citations: selected
            .iter()
            .map(|(_, candidate)| candidate.hit.citation.clone())
            .collect(),
        context: explain_context(ranked, &selected, context),
        candidate_trace_limit: CANDIDATE_TRACE_LIMIT,
        candidate_trace_truncated: truncated,
        llm_skipped: true,
    }
}

fn candidate_trace(
    candidate: &RankedCandidate,
    score_kind: AskExplainScoreKind,
) -> AskExplainCandidate {
    let selected = candidate.selected_context_rank.is_some();
    AskExplainCandidate {
        id: candidate.hit.chunk_id.clone(),
        url: candidate.hit.canonical_uri.clone(),
        chunk_index: None,
        raw_rerank_rank: candidate.rerank_rank,
        planned_full_doc_rank: None,
        selected_context_rank: candidate.selected_context_rank,
        insertion_mode: Some(if selected {
            AskExplainInsertionMode::TopChunk
        } else {
            AskExplainInsertionMode::NotSelected
        }),
        retrieval_score: candidate.hit.score,
        rerank_score: candidate.rerank_score,
        score_kind,
        score_components: candidate.score_components.clone(),
        filter_decisions: candidate.filter_decisions.clone(),
        selection_decisions: vec![selection_decision(candidate)],
        snippet: snippet(&candidate.hit.text),
    }
}

fn selection_decision(candidate: &RankedCandidate) -> AskExplainSelectionDecision {
    if candidate.selected_context_rank.is_some() {
        return AskExplainSelectionDecision {
            kind: AskExplainSelectionDecisionKind::SelectedTopChunk,
            reason: candidate.selection_reason.clone(),
        };
    }
    let skipped_budget = candidate
        .selection_reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with("skipped:"));
    AskExplainSelectionDecision {
        kind: if skipped_budget {
            AskExplainSelectionDecisionKind::SkippedBudget
        } else {
            AskExplainSelectionDecisionKind::NotSelected
        },
        reason: candidate.selection_reason.clone().or_else(|| {
            candidate
                .filter_decisions
                .iter()
                .find(|decision| decision.kind != AskExplainFilterDecisionKind::Kept)
                .and_then(|decision| decision.reason.clone())
        }),
    }
}

fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.char_indices().nth(SNIPPET_MAX_CHARS) {
        Some((byte_index, _)) => format!("{}…", &trimmed[..byte_index]),
        None => trimmed.to_string(),
    }
}

fn explain_context(
    ranked: &RankingResult,
    selected: &[(usize, &RankedCandidate)],
    context: &str,
) -> AskExplainContext {
    let final_source_order = selected
        .iter()
        .map(|(rank, candidate)| AskExplainContextSource {
            source_id: format!("S{rank}"),
            url: candidate.hit.canonical_uri.clone(),
            tier: axon_core::ask_explain::AskExplainContextSourceTier::TopChunk,
            sort_rank: rank.saturating_sub(1),
            sort_score: candidate.rerank_score,
        })
        .collect();
    let truncated_by_budget = ranked.candidates.iter().any(|candidate| {
        candidate.kept()
            && candidate.selected_context_rank.is_none()
            && candidate
                .selection_reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("skipped:"))
    });

    AskExplainContext {
        planned_full_doc_urls: Vec::new(),
        full_doc_fetch_errors: Vec::new(),
        full_doc_fetch_skipped: true,
        full_doc_fetch_skip_reason: AskExplainFullDocFetchSkipReason::Disabled,
        full_doc_fetch_mode: AskExplainFullDocFetchMode::Rrf,
        final_source_order,
        context_char_budget: ranked.effective_budget.max_context_chars,
        context_chars_used: context.chars().count(),
        context_bytes_budget: ranked.effective_budget.max_context_chars,
        context_bytes_used: context.len(),
        rendered_context: None,
        truncated_by_budget,
    }
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;

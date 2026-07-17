//! Builds `ask --explain`'s [`AskExplainTrace`] from the `axon-retrieval`
//! engine's hybrid RRF hits.
//!
//! Issue #298 finale: the legacy `axon_vector` reranker (and the whole
//! `axon-vector` crate) is retired, so `train`'s tuning signal now reflects
//! the actual production retrieval engine instead of a dead code path.
//!
//! The legacy reranker computed a much richer trace: per-candidate
//! rerank-score deltas from several scoring heuristics, planned/inserted
//! full-doc staging, supplemental-chunk backfill, and topical-overlap/
//! duplicate filtering (see the now-deleted
//! `axon-vector/src/ops/commands/ask/context.rs`). None of that exists in
//! `axon-retrieval`'s single dense+bm42 RRF hybrid search
//! (`crates/axon-retrieval/src/engine.rs`), so this trace is intentionally
//! narrower — and that narrowing is the accepted, contract-aligned outcome of
//! this cutover, not an oversight:
//!
//! - `retrieval_score` and `rerank_score` are always equal (the fused RRF
//!   score) — there is no separate rerank stage.
//! - `chunk_index`, `planned_full_doc_rank`, and full-doc-fetch bookkeeping
//!   are always `None`/empty — the retrieval-engine `ask` path selects
//!   top-chunks only (see
//!   [`super::super::synthesis::AskContext::from_retrieval`]).
//! - `filter_decisions`/`selection_decisions` reduce to a single binary: a
//!   hit that landed in the rendered context (a strict-prefix cutoff of the
//!   retrieval-ordered hit list — see `build_ask_context_from_hits`) is
//!   `Kept`/`SelectedTopChunk`; everything else is `DroppedLowSignal`/
//!   `NotSelected`.
//! - There is no dual NL/keyword query embedding at this layer (see
//!   `crates/axon-retrieval/src/engine.rs::retrieve`), so `keyword_query`
//!   mirrors `query` and `dual_search` is always `false`.
//!
//! `train` (`axon-cli/src/commands/train.rs`) only reads `filter_decisions`
//! (filtering for `Kept`), `rerank_score`/`retrieval_score`, `url`,
//! `chunk_index`, and `snippet` — every field it needs is populated here.

use axon_core::ask_explain::{
    AskExplainCandidate, AskExplainContext, AskExplainContextSource, AskExplainFilterDecision,
    AskExplainFilterDecisionKind, AskExplainFullDocFetchMode, AskExplainFullDocFetchSkipReason,
    AskExplainInsertionMode, AskExplainMode, AskExplainRetrieval, AskExplainScoreComponent,
    AskExplainScoreComponentStatus, AskExplainScoreKind, AskExplainSelectionDecision,
    AskExplainSelectionDecisionKind, AskExplainTrace,
};
use axon_core::config::Config;
use axon_retrieval::QueryServiceHit;

/// Cap on the number of candidates included in the trace. Matches the legacy
/// reranker's `ASK_EXPLAIN_CANDIDATE_TRACE_LIMIT` so `train`'s downstream
/// expectations about trace size don't shift.
const CANDIDATE_TRACE_LIMIT: usize = 50;

/// Longest chunk-text snippet included per candidate in the trace.
const SNIPPET_MAX_CHARS: usize = 280;

/// Build the `ask --explain` trace from the retrieval engine's ranked hits.
///
/// `hits` is the full candidate pool returned by `run_query`, in retrieval
/// order. `chunks_selected` is the count of hits (from the front of that same
/// list) that were rendered into `context` — `build_ask_context_from_hits`
/// selects a strict prefix of `hits` (capped by `cfg.ask_chunk_limit` and the
/// context-byte budget), so `hits[..chunks_selected]` is exactly the selected
/// set.
pub(crate) fn build_explain_trace(
    cfg: &Config,
    question: &str,
    hits: &[QueryServiceHit],
    chunks_selected: usize,
    context: &str,
) -> AskExplainTrace {
    let candidate_limit = hits.len();
    let truncated = candidate_limit > CANDIDATE_TRACE_LIMIT;
    let candidates = hits
        .iter()
        .take(CANDIDATE_TRACE_LIMIT)
        .enumerate()
        .map(|(idx, hit)| candidate_trace(hit, idx, idx < chunks_selected))
        .collect();

    AskExplainTrace {
        mode: AskExplainMode::ExplainOnly,
        retrieval: AskExplainRetrieval {
            query: question.to_string(),
            keyword_query: question.to_string(),
            dual_search: false,
            collection: cfg.collection.clone(),
            candidate_limit,
            hybrid_search_enabled: true,
            hybrid_candidate_limit: cfg.ask_hybrid_candidates,
            score_kind: AskExplainScoreKind::Rrf,
            vector_mode: "named_hybrid_rrf".to_string(),
            sparse_query_status: None,
        },
        candidates,
        citations: hits
            .iter()
            .take(chunks_selected)
            .map(|hit| hit.citation.clone())
            .collect(),
        context: explain_context(hits, chunks_selected, context, cfg.ask_max_context_chars),
        candidate_trace_limit: CANDIDATE_TRACE_LIMIT,
        candidate_trace_truncated: truncated,
        llm_skipped: true,
    }
}

fn candidate_trace(hit: &QueryServiceHit, idx: usize, selected: bool) -> AskExplainCandidate {
    let rank = idx + 1;
    AskExplainCandidate {
        id: hit.chunk_id.clone(),
        url: hit.canonical_uri.clone(),
        chunk_index: None,
        raw_rerank_rank: Some(rank),
        planned_full_doc_rank: None,
        selected_context_rank: selected.then_some(rank),
        insertion_mode: Some(if selected {
            AskExplainInsertionMode::TopChunk
        } else {
            AskExplainInsertionMode::NotSelected
        }),
        retrieval_score: hit.score,
        rerank_score: hit.score,
        score_kind: AskExplainScoreKind::Rrf,
        score_components: vec![AskExplainScoreComponent {
            name: "hybrid_rrf".to_string(),
            value: hit.score,
            status: AskExplainScoreComponentStatus::Applied,
            reason: None,
        }],
        filter_decisions: vec![filter_decision(selected)],
        selection_decisions: vec![selection_decision(selected)],
        snippet: snippet(&hit.text),
    }
}

fn filter_decision(selected: bool) -> AskExplainFilterDecision {
    if selected {
        AskExplainFilterDecision {
            kind: AskExplainFilterDecisionKind::Kept,
            reason: None,
        }
    } else {
        AskExplainFilterDecision {
            kind: AskExplainFilterDecisionKind::DroppedLowSignal,
            reason: Some(
                "not selected: ranked beyond the chunk-limit/context-budget cutoff".to_string(),
            ),
        }
    }
}

fn selection_decision(selected: bool) -> AskExplainSelectionDecision {
    AskExplainSelectionDecision {
        kind: if selected {
            AskExplainSelectionDecisionKind::SelectedTopChunk
        } else {
            AskExplainSelectionDecisionKind::NotSelected
        },
        reason: None,
    }
}

/// Truncate chunk text to a short human-scannable snippet for the trace.
///
/// The legacy reranker centered snippets on query-token matches; the new
/// engine's hits are already relevance-ranked by the vector store's hybrid
/// search, so a simple char-boundary head truncation is a reasonable
/// stand-in without re-deriving query tokenization here.
fn snippet(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.char_indices().nth(SNIPPET_MAX_CHARS) {
        Some((byte_idx, _)) => format!("{}…", &trimmed[..byte_idx]),
        None => trimmed.to_string(),
    }
}

/// Build the `context` portion of the trace. Full-doc fetch is a legacy-only
/// concept on this path (always skipped/disabled); `rendered_context` is left
/// `None`, matching the legacy production path (it was only ever populated in
/// legacy's own tests, never at runtime).
fn explain_context(
    hits: &[QueryServiceHit],
    chunks_selected: usize,
    context: &str,
    max_context_chars: usize,
) -> AskExplainContext {
    let final_source_order = hits
        .iter()
        .take(chunks_selected)
        .enumerate()
        .map(|(idx, hit)| AskExplainContextSource {
            source_id: format!("S{}", idx + 1),
            url: hit.canonical_uri.clone(),
            tier: axon_core::ask_explain::AskExplainContextSourceTier::TopChunk,
            sort_rank: idx,
            sort_score: hit.score,
        })
        .collect();

    AskExplainContext {
        planned_full_doc_urls: Vec::new(),
        full_doc_fetch_errors: Vec::new(),
        full_doc_fetch_skipped: true,
        full_doc_fetch_skip_reason: AskExplainFullDocFetchSkipReason::Disabled,
        full_doc_fetch_mode: AskExplainFullDocFetchMode::Rrf,
        final_source_order,
        context_char_budget: max_context_chars,
        context_chars_used: context.chars().count(),
        context_bytes_budget: max_context_chars,
        context_bytes_used: context.len(),
        rendered_context: None,
        truncated_by_budget: chunks_selected < hits.len(),
    }
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;

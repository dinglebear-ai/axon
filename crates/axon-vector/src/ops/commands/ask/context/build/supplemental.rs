use super::appenders::append_supplemental_chunks;
use super::selection::collect_supplemental_candidate_indices;
use crate::ops::commands::ask::context::heuristics::should_inject_supplemental;
use crate::ops::ranking;
use std::collections::HashSet;

/// Run the supplemental backfill pass when coverage is thin and budget allows.
/// Extracted from context assembly to keep build.rs below monolith limits.
#[allow(clippy::too_many_arguments)]
pub(super) fn maybe_inject_supplemental(
    reranked: &[ranking::AskCandidate],
    inserted_full_doc_urls: &HashSet<String>,
    min_supplemental_score: Option<f64>,
    full_docs_selected: usize,
    top_chunks_selected: usize,
    backfill_limit: usize,
    max_context_chars: usize,
    separator: &str,
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    source_idx: &mut usize,
) -> (Vec<usize>, usize) {
    if !should_inject_supplemental(
        *context_char_count,
        max_context_chars,
        full_docs_selected,
        top_chunks_selected,
    ) {
        return (Vec::new(), 0);
    }
    let supplemental_candidate_indices = collect_supplemental_candidate_indices(
        reranked,
        inserted_full_doc_urls,
        min_supplemental_score,
    );
    let supplemental = ranking::select_diverse_candidates_from_indices(
        reranked,
        &supplemental_candidate_indices,
        backfill_limit,
        1,
    );
    let supplemental_count = append_supplemental_chunks(
        reranked,
        &supplemental,
        context_entries,
        context_char_count,
        source_idx,
        separator,
        max_context_chars,
    );
    (supplemental, supplemental_count)
}

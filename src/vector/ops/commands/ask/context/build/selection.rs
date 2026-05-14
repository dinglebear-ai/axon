use crate::vector::ops::ranking;
use std::collections::HashSet;

pub fn select_context_indices(
    reranked: &[ranking::AskCandidate],
    chunk_limit: usize,
    full_doc_limit: usize,
) -> (Vec<usize>, Vec<usize>) {
    let top_chunk_indices = ranking::select_diverse_candidates(reranked, chunk_limit, 1);
    // Full-doc indices are selected independently from the full reranked pool.
    // The old URL-exclusion caused top_full_doc_indices=[] for narrow-domain
    // queries (all top URLs already in chunk slots), silently skipping the
    // full-doc Qdrant fetch (context_build_ms ≈ 5ms).
    // append_top_chunks_to_context at line 219 already skips snippet entries
    // for URLs in planned_full_doc_urls — no duplication occurs.
    // Enable ask_fulldoc_skip_enabled to restore fast-path when top chunks
    // already provide sufficient coverage.
    let top_full_doc_indices = ranking::select_diverse_candidates(reranked, full_doc_limit, 1);
    (top_chunk_indices, top_full_doc_indices)
}

pub fn planned_full_doc_urls(
    reranked: &[ranking::AskCandidate],
    top_full_doc_indices: &[usize],
    skip_full_doc_fetch: bool,
) -> HashSet<String> {
    if skip_full_doc_fetch {
        return HashSet::new();
    }

    top_full_doc_indices
        .iter()
        .filter_map(|&idx| reranked.get(idx).map(|candidate| candidate.url.clone()))
        .collect()
}

pub fn collect_supplemental_candidate_indices(
    reranked: &[ranking::AskCandidate],
    inserted_full_doc_urls: &HashSet<String>,
    min_supplemental_score: Option<f64>,
) -> Vec<usize> {
    reranked
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            !inserted_full_doc_urls.contains(&candidate.url)
                && min_supplemental_score.is_none_or(|floor| candidate.rerank_score >= floor)
        })
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>()
}

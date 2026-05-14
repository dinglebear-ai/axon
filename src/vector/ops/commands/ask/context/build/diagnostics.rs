use crate::vector::ops::ranking;
use crate::vector::ops::source_display::display_source;
use std::collections::HashSet;

pub fn build_diagnostic_sources(
    reranked: &[ranking::AskCandidate],
    top_chunk_indices: &[usize],
    top_chunks_selected: usize,
    planned_full_doc_urls: &HashSet<String>,
    top_full_doc_indices: &[usize],
    supplemental: &[usize],
    supplemental_count: usize,
) -> Vec<String> {
    let mut diagnostic_sources: Vec<String> = Vec::new();
    diagnostic_sources.extend(
        top_chunk_indices
            .iter()
            .map(|&idx| &reranked[idx])
            .filter(|candidate| !planned_full_doc_urls.contains(&candidate.url))
            .take(top_chunks_selected)
            .map(diagnostic_chunk_source),
    );
    diagnostic_sources.extend(
        top_full_doc_indices
            .iter()
            .map(|&idx| &reranked[idx])
            .map(diagnostic_full_doc_source),
    );
    diagnostic_sources.extend(
        supplemental
            .iter()
            .map(|&idx| &reranked[idx])
            .take(supplemental_count)
            .map(diagnostic_chunk_source),
    );
    diagnostic_sources
}

fn diagnostic_chunk_source(candidate: &ranking::AskCandidate) -> String {
    format!(
        "chunk rerank_score={:.3} retrieval_score={:.3} url={}",
        candidate.rerank_score,
        candidate.score,
        display_source(&candidate.url)
    )
}

fn diagnostic_full_doc_source(candidate: &ranking::AskCandidate) -> String {
    format!(
        "full-doc rerank_score={:.3} retrieval_score={:.3} url={}",
        candidate.rerank_score,
        candidate.score,
        display_source(&candidate.url)
    )
}

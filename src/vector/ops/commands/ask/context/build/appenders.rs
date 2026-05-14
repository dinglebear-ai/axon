use super::super::heuristics::push_context_entry;
use crate::vector::ops::source_display::display_source;
use crate::vector::ops::{qdrant, ranking};
use std::collections::HashSet;

fn renumber_context_source_header(entry: &str, display_id: usize) -> String {
    let Some(start) = entry.find("[S") else {
        return entry.to_string();
    };
    let rest = &entry[start + 2..];
    let Some(end_rel) = rest.find(']') else {
        return entry.to_string();
    };
    if rest[..end_rel].parse::<usize>().is_err() {
        return entry.to_string();
    }
    let end = start + 2 + end_rel;
    format!("{}S{}{}", &entry[..start + 1], display_id, &entry[end..])
}

#[allow(clippy::too_many_arguments)]
pub fn append_top_chunks_to_context(
    reranked: &[ranking::AskCandidate],
    top_chunk_indices: &[usize],
    planned_full_doc_urls: &HashSet<String>,
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    source_idx: &mut usize,
    separator: &str,
    max_context_chars: usize,
) -> usize {
    let mut top_chunks_selected = 0usize;
    for &chunk_idx in top_chunk_indices {
        let chunk = &reranked[chunk_idx];
        if planned_full_doc_urls.contains(&chunk.url) {
            continue;
        }
        let source = display_source(&chunk.url);
        let entry = format!(
            "## Top Chunk [S{}]: {}\n\n{}",
            *source_idx, source, chunk.chunk_text
        );
        if !push_context_entry(
            context_entries,
            context_char_count,
            chunk.rerank_score,
            entry,
            separator,
            max_context_chars,
        ) {
            break;
        }
        top_chunks_selected += 1;
        *source_idx += 1;
    }
    top_chunks_selected
}

/// Number of chunks per fetched full-doc that survive the query-relevance
/// filter before being concatenated. Tradeoff: small enough to drop irrelevant
/// chunks, large enough to preserve narrative context. (bd axon_rust-0fz)
const FULL_DOC_RENDER_TOP_K: usize = 24;

#[allow(clippy::too_many_arguments)]
pub fn append_full_docs_to_context(
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    inserted_full_doc_urls: &mut HashSet<String>,
    mut source_idx: usize,
    separator: &str,
    max_context_chars: usize,
    fetched_docs: Vec<(usize, String, Vec<qdrant::QdrantPoint>)>,
    query_tokens: &[String],
    url_to_score: &std::collections::HashMap<String, f64>,
) -> (usize, usize) {
    let mut full_docs_selected = 0usize;
    for (_idx, url, points) in fetched_docs {
        let text = qdrant::render_full_doc_filtered(
            points,
            Some(query_tokens),
            Some(FULL_DOC_RENDER_TOP_K),
        );
        if text.is_empty() {
            continue;
        }
        let source = display_source(&url);
        let entry = format!(
            "## Source Document [S{}]: {}\n\n{}",
            source_idx, source, text
        );
        let score = url_to_score.get(&url).copied().unwrap_or(0.0);
        if !push_context_entry(
            context_entries,
            context_char_count,
            score,
            entry,
            separator,
            max_context_chars,
        ) {
            break;
        }
        inserted_full_doc_urls.insert(url);
        full_docs_selected += 1;
        source_idx += 1;
    }
    (full_docs_selected, source_idx)
}

pub fn append_supplemental_chunks(
    reranked: &[ranking::AskCandidate],
    supplemental: &[usize],
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    source_idx: &mut usize,
    separator: &str,
    max_context_chars: usize,
) -> usize {
    let mut supplemental_count = 0usize;
    for &chunk_idx in supplemental {
        let chunk = &reranked[chunk_idx];
        let source = display_source(&chunk.url);
        let entry = format!(
            "## Supplemental Chunk [S{}]: {}\n\n{}",
            *source_idx, source, chunk.chunk_text
        );
        if !push_context_entry(
            context_entries,
            context_char_count,
            chunk.rerank_score,
            entry,
            separator,
            max_context_chars,
        ) {
            break;
        }
        supplemental_count += 1;
        *source_idx += 1;
    }
    supplemental_count
}

#[cfg(test)]
mod renumber_tests {
    use super::renumber_context_source_header;

    #[test]
    fn renumber_context_source_header_updates_existing_source_id() {
        let entry = "## Top Chunk [S11]: https://docs.example.com\n\nbody";
        assert_eq!(
            renumber_context_source_header(entry, 1),
            "## Top Chunk [S1]: https://docs.example.com\n\nbody"
        );
    }

    #[test]
    fn renumber_context_source_header_leaves_malformed_header_unchanged() {
        let entry = "## Top Chunk [SX]: https://docs.example.com\n\nbody";
        assert_eq!(renumber_context_source_header(entry, 1), entry);
    }
}

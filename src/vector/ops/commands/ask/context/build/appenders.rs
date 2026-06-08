use super::super::heuristics::push_context_entry;
use crate::vector::ops::source_display::display_source;
use crate::vector::ops::{qdrant, ranking};
use std::collections::HashSet;

#[cfg(test)]
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
        let source = display_source(&url);
        let Some(entry) = fit_full_doc_entry_to_budget(
            &source,
            source_idx,
            &points,
            query_tokens,
            *context_char_count,
            separator,
            max_context_chars,
        ) else {
            continue;
        };
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

fn fit_full_doc_entry_to_budget(
    source: &str,
    source_idx: usize,
    points: &[qdrant::QdrantPoint],
    query_tokens: &[String],
    context_char_count: usize,
    separator: &str,
    max_context_chars: usize,
) -> Option<String> {
    for top_k in [FULL_DOC_RENDER_TOP_K, 16, 12, 8, 4, 2, 1] {
        let text =
            qdrant::render_full_doc_filtered(points.to_vec(), Some(query_tokens), Some(top_k));
        if text.is_empty() {
            continue;
        }
        let entry = source_document_entry(source_idx, source, &text, false);
        if entry_fits_context(&entry, context_char_count, separator, max_context_chars) {
            return Some(entry);
        }
    }

    let text = qdrant::render_full_doc_filtered(points.to_vec(), Some(query_tokens), Some(1));
    let header = format!("## Source Document [S{}]: {}\n\n", source_idx, source);
    let marker = "\n\n[Excerpt truncated to fit the context budget.]";
    let available = remaining_entry_chars(context_char_count, separator, max_context_chars)?;
    if available <= header.len() + marker.len() {
        return None;
    }
    let body_budget = available - header.len() - marker.len();
    let body = truncate_to_char_boundary(text.trim(), body_budget);
    if body.trim().is_empty() {
        return None;
    }
    Some(format!("{header}{body}{marker}"))
}

fn source_document_entry(source_idx: usize, source: &str, text: &str, truncated: bool) -> String {
    let marker = if truncated {
        "\n\n[Excerpt truncated to fit the context budget.]"
    } else {
        ""
    };
    format!(
        "## Source Document [S{}]: {}\n\n{}{}",
        source_idx, source, text, marker
    )
}

fn entry_fits_context(
    entry: &str,
    context_char_count: usize,
    separator: &str,
    max_context_chars: usize,
) -> bool {
    remaining_entry_chars(context_char_count, separator, max_context_chars)
        .is_some_and(|remaining| entry.len() <= remaining)
}

fn remaining_entry_chars(
    context_char_count: usize,
    separator: &str,
    max_context_chars: usize,
) -> Option<usize> {
    let sep_len = if context_char_count == 0 {
        0
    } else {
        separator.len()
    };
    max_context_chars.checked_sub(context_char_count + sep_len)
}

fn truncate_to_char_boundary(text: &str, max_len: usize) -> &str {
    if text.len() <= max_len {
        return text;
    }
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
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
#[path = "appenders_renumber_tests.rs"]
mod renumber_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::ops::qdrant::{QdrantPayload, QdrantPoint};
    use std::collections::{HashMap, HashSet};

    fn point(text: &str, chunk_index: i64) -> QdrantPoint {
        QdrantPoint {
            id: serde_json::Value::Null,
            payload: QdrantPayload {
                url: "https://docs.example.com/storage/redundancy".to_string(),
                chunk_text: text.to_string(),
                text: String::new(),
                chunk_index: Some(chunk_index),
            },
        }
    }

    #[test]
    fn full_doc_insertion_keeps_bounded_excerpt_when_relevant_doc_exceeds_budget() {
        let large_relevant = format!(
            "pool risk reason special vdev redundancy {}",
            "details ".repeat(120)
        );
        let fetched_docs = vec![(
            0,
            "https://docs.example.com/storage/redundancy".to_string(),
            vec![point(&large_relevant, 0)],
        )];
        let mut context_entries = Vec::new();
        let mut context_char_count = 0usize;
        let mut inserted = HashSet::new();
        let query_tokens = vec!["pool".to_string(), "risk".to_string(), "reason".to_string()];
        let url_to_score = HashMap::from([(
            "https://docs.example.com/storage/redundancy".to_string(),
            1.0,
        )]);

        let (selected, _) = append_full_docs_to_context(
            &mut context_entries,
            &mut context_char_count,
            &mut inserted,
            1,
            "\n\n---\n\n",
            280,
            fetched_docs,
            &query_tokens,
            &url_to_score,
        );

        assert_eq!(selected, 1);
        assert_eq!(
            inserted,
            HashSet::from(["https://docs.example.com/storage/redundancy".to_string()])
        );
        assert_eq!(context_entries.len(), 1);
        assert!(
            context_entries[0].1.contains("pool risk reason"),
            "bounded excerpt should preserve the relevant explanation text"
        );
        assert!(context_char_count <= 280);
    }
}

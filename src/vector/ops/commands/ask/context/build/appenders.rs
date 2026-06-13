use super::super::heuristics::push_context_entry;
use crate::vector::ops::source_display::display_source;
use crate::vector::ops::{qdrant, ranking};
use std::collections::HashSet;
use std::sync::Arc;

const CONTEXT_PREFIX_LEN: usize = "Sources:\n".len();

/// Byte overhead added to every retrieved-content entry by the
/// `<retrieved_content trust="evidence_only">` XML boundary.
/// Used to adjust budget estimates so they remain correct after wrapping.
pub(super) const XML_WRAPPER_OVERHEAD: usize =
    "<retrieved_content trust=\"evidence_only\">\n".len() + "\n</retrieved_content>".len();

/// Wrap the body of a retrieved chunk in an XML trust boundary and return
/// the full entry string including the axon-generated `header`.
///
/// The boundary tells the synthesis model that the enclosed content is
/// untrusted indexed data, not axon-emitted scaffolding.  Pairs with
/// `defang_chunk_text` which breaks structural markers *inside* the body.
fn wrap_retrieved_content(header: &str, body: &str) -> String {
    format!("{header}<retrieved_content trust=\"evidence_only\">\n{body}\n</retrieved_content>")
}

/// Defang structural markers that the ask synthesis prompt treats as
/// axon-generated context scaffolding.  Prevents indexed content from
/// injecting forged citation keys (`[S{n}]`) or source-section headers
/// (`## Sources`, `## Source Document`, etc.) into the synthesis context.
///
/// Uses a zero-width space (U+200B) to break marker recognition without
/// altering the visible text rendered to a user.
pub(super) fn defang_chunk_text(text: &str) -> String {
    // Break markdown headers that match axon's own context markers.
    let s = text
        .replace("## Sources", "## \u{200b}Sources")
        .replace("## Source Document", "## \u{200b}Source Document")
        .replace("## Top Chunk", "## \u{200b}Top Chunk")
        .replace("## Supplemental Chunk", "## \u{200b}Supplemental Chunk");
    // Break [S{digits}] citation-like patterns.
    defang_citation_patterns(&s)
}

/// Replace `[S{digits}]` with `[​S{digits}]` (zero-width space after `[`)
/// so indexed content cannot inject forged citation references.
fn defang_citation_patterns(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(pos) = rest.find("[S") {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos + 2..]; // content after "[S"
        let digit_end = tail.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digit_end > 0 && tail[digit_end..].starts_with(']') {
            // Matched [S{digits}] — insert zero-width space after '['
            result.push_str("[\u{200b}S");
            result.push_str(&tail[..digit_end]);
            result.push(']');
            rest = &tail[digit_end + 1..];
        } else {
            result.push_str("[S");
            rest = tail;
        }
    }
    result.push_str(rest);
    result
}

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
        let header = format!("## Top Chunk [S{}]: {}\n\n", *source_idx, source);
        let body = defang_chunk_text(&chunk.chunk_text);
        let entry = wrap_retrieved_content(&header, &body);
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

/// Initial maximum chunks per fetched full-doc before fallback attempts try
/// smaller windows/excerpts to keep later docs from disappearing on budget.
const FULL_DOC_RENDER_TOP_K: usize = 24;
const FULL_DOC_CONTEXT_TIER_BOOST: f64 = 10.0;

#[allow(clippy::too_many_arguments)]
pub fn append_full_docs_to_context(
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    inserted_full_doc_urls: &mut HashSet<String>,
    mut source_idx: usize,
    separator: &str,
    max_context_chars: usize,
    fetched_docs: Vec<(usize, String, Arc<Vec<qdrant::QdrantPoint>>)>,
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
        let score = url_to_score.get(&url).copied().unwrap_or(0.0) + FULL_DOC_CONTEXT_TIER_BOOST;
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
    let available = remaining_entry_chars(context_char_count, separator, max_context_chars)?;
    let header = format!("## Source Document [S{}]: {}\n\n", source_idx, source);

    // Rank the chunks once, then walk the top-k ladder on length arithmetic —
    // the previous loop cloned the points and re-scored + re-rendered the
    // whole document on every rung (up to 7 full renders per doc on the hot
    // ask path). Only the rung that fits is actually rendered.
    let order = qdrant::rank_points_by_query_overlap(points, query_tokens);
    let mut prev_k = usize::MAX;
    for top_k in [FULL_DOC_RENDER_TOP_K, 16, 12, 8, 4, 2, 1] {
        let k = top_k.min(order.len());
        if k == 0 || k == prev_k {
            continue;
        }
        prev_k = k;
        // Conservative upper bound on the rendered size — an estimate that fits
        // guarantees the real render fits.  XML_WRAPPER_OVERHEAD is included so
        // the estimate stays accurate after content wrapping.
        let estimate: usize = order[..k]
            .iter()
            .map(|&i| {
                let text = qdrant::payload_text_typed(&points[i].payload);
                if text.is_empty() { 0 } else { text.len() + 1 }
            })
            .sum();
        if estimate == 0 {
            break;
        }
        if header.len() + XML_WRAPPER_OVERHEAD + estimate <= available {
            let raw_text = qdrant::render_points_in_doc_order(points, &order[..k]);
            if raw_text.is_empty() {
                break;
            }
            let text = defang_chunk_text(&raw_text);
            return Some(source_document_entry(source_idx, source, &text, false));
        }
    }

    let marker = "\n\n[Excerpt truncated to fit the context budget.]";
    // Available must exceed header + XML wrapper + at least one content char + marker.
    if available <= header.len() + XML_WRAPPER_OVERHEAD + marker.len() {
        return None;
    }
    let smallest_text = if order.is_empty() {
        String::new()
    } else {
        qdrant::render_points_in_doc_order(points, &order[..1])
    };
    let body_budget = available - header.len() - XML_WRAPPER_OVERHEAD - marker.len();
    let raw_body = truncate_to_char_boundary(smallest_text.trim(), body_budget);
    if raw_body.trim().is_empty() {
        return None;
    }
    let body = defang_chunk_text(raw_body);
    Some(source_document_entry(
        source_idx,
        source,
        &format!("{body}{marker}"),
        true,
    ))
}

fn source_document_entry(source_idx: usize, source: &str, body: &str, _truncated: bool) -> String {
    let header = format!("## Source Document [S{}]: {}\n\n", source_idx, source);
    wrap_retrieved_content(&header, body)
}

fn remaining_entry_chars(
    context_char_count: usize,
    separator: &str,
    max_context_chars: usize,
) -> Option<usize> {
    let sep_len = if context_char_count == 0 || context_char_count == CONTEXT_PREFIX_LEN {
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
        let header = format!("## Supplemental Chunk [S{}]: {}\n\n", *source_idx, source);
        let body = defang_chunk_text(&chunk.chunk_text);
        let entry = wrap_retrieved_content(&header, &body);
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
#[path = "appenders_tests.rs"]
mod tests;

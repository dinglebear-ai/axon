use super::super::heuristics::SkipDecision;
use super::{
    CONTEXT_PREFIX, ContextCandidateSelection, ContextSelectionInputs,
    build_context_selection_decisions, build_diagnostic_sources, context_source_candidate_count,
    final_source_order_from_entries, sorted_urls,
};
use crate::services::types::{
    AskExplainContext, AskExplainFullDocFetchMode, AskExplainFullDocFetchSkipReason,
};
use crate::vector::ops::ranking;
use anyhow::{Result, anyhow};
use std::collections::HashSet;

pub(super) struct FinalizeContextInputs<'a> {
    pub(super) reranked: &'a [ranking::AskCandidate],
    pub(super) top_chunk_indices: &'a [usize],
    pub(super) top_full_doc_indices: &'a [usize],
    pub(super) selected_top_chunk_indices: &'a [usize],
    pub(super) planned_full_doc_urls_set: &'a HashSet<String>,
    pub(super) inserted_full_doc_urls: &'a HashSet<String>,
    pub(super) supplemental: &'a [usize],
    pub(super) supplemental_count: usize,
    pub(super) top_chunks_selected: usize,
    pub(super) full_docs_selected: usize,
    pub(super) max_context_chars: usize,
    pub(super) skip_decision: SkipDecision,
    pub(super) is_rrf: bool,
    pub(super) separator: &'a str,
    pub(super) context_started: std::time::Instant,
    pub(super) context_entries: Vec<(f64, String)>,
}

pub(super) struct FinalizedAskContext {
    pub(super) context: String,
    pub(super) context_elapsed_ms: u128,
    pub(super) diagnostic_sources: Vec<String>,
    pub(super) explain_context: AskExplainContext,
    pub(super) selection_decisions: Vec<ContextCandidateSelection>,
}

pub(super) fn finalize_built_context(
    mut inputs: FinalizeContextInputs<'_>,
) -> Result<FinalizedAskContext> {
    if inputs.context_entries.is_empty() {
        return Err(anyhow!("Failed to retrieve any context sources for ask"));
    }

    // Flatten by rerank_score across all buckets (top-chunks/full-docs/supplemental):
    // LLMs have proximity bias, so highest-scoring chunks should appear first.
    inputs
        .context_entries
        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let final_source_order = final_source_order_from_entries(&inputs.context_entries);
    let joined = inputs
        .context_entries
        .iter()
        .enumerate()
        .map(|(idx, (_, entry))| renumber_source_header(entry, idx + 1))
        .collect::<Vec<_>>();
    let context = format!("{CONTEXT_PREFIX}{}", joined.join(inputs.separator));
    let explain_context = build_explain_context(
        &context,
        ExplainContextInputs {
            reranked: inputs.reranked,
            top_chunk_indices: inputs.top_chunk_indices,
            top_full_doc_indices: inputs.top_full_doc_indices,
            selected_top_chunk_indices: inputs.selected_top_chunk_indices,
            planned_full_doc_urls_set: inputs.planned_full_doc_urls_set,
            inserted_full_doc_urls: inputs.inserted_full_doc_urls,
            supplemental: inputs.supplemental,
            supplemental_count: inputs.supplemental_count,
            full_docs_selected: inputs.full_docs_selected,
            max_context_chars: inputs.max_context_chars,
            skip_decision: inputs.skip_decision,
            is_rrf: inputs.is_rrf,
            final_source_order,
        },
    );
    let selection_decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: inputs.reranked,
        top_chunk_indices: inputs.top_chunk_indices,
        selected_top_chunk_indices: inputs.selected_top_chunk_indices,
        planned_full_doc_urls: inputs.planned_full_doc_urls_set,
        top_full_doc_indices: inputs.top_full_doc_indices,
        inserted_full_doc_urls: inputs.inserted_full_doc_urls,
        supplemental_indices: inputs.supplemental,
        supplemental_count: inputs.supplemental_count,
        full_doc_fetch_skipped: inputs.skip_decision.skip,
        final_source_order: &explain_context.final_source_order,
    });

    Ok(FinalizedAskContext {
        context,
        context_elapsed_ms: inputs.context_started.elapsed().as_millis(),
        diagnostic_sources: build_diagnostic_sources(
            inputs.reranked,
            inputs.top_chunk_indices,
            inputs.top_chunks_selected,
            inputs.inserted_full_doc_urls,
            inputs.top_full_doc_indices,
            inputs.supplemental,
            inputs.supplemental_count,
        ),
        explain_context,
        selection_decisions,
    })
}

struct ExplainContextInputs<'a> {
    reranked: &'a [ranking::AskCandidate],
    top_chunk_indices: &'a [usize],
    top_full_doc_indices: &'a [usize],
    selected_top_chunk_indices: &'a [usize],
    planned_full_doc_urls_set: &'a HashSet<String>,
    inserted_full_doc_urls: &'a HashSet<String>,
    supplemental: &'a [usize],
    supplemental_count: usize,
    full_docs_selected: usize,
    max_context_chars: usize,
    skip_decision: SkipDecision,
    is_rrf: bool,
    final_source_order: Vec<crate::services::types::AskExplainContextSource>,
}

fn build_explain_context(context: &str, inputs: ExplainContextInputs<'_>) -> AskExplainContext {
    let truncated_by_budget = inputs.selected_top_chunk_indices.len()
        + inputs.full_docs_selected
        + inputs.supplemental_count
        < context_source_candidate_count(
            inputs.reranked,
            inputs.top_chunk_indices,
            inputs.inserted_full_doc_urls,
            inputs.top_full_doc_indices,
            inputs.supplemental,
            inputs.skip_decision.skip,
        );
    AskExplainContext {
        planned_full_doc_urls: sorted_urls(inputs.planned_full_doc_urls_set),
        full_doc_fetch_skipped: inputs.skip_decision.skip,
        full_doc_fetch_skip_reason: AskExplainFullDocFetchSkipReason::from(
            inputs.skip_decision.reason,
        ),
        full_doc_fetch_mode: if inputs.is_rrf {
            AskExplainFullDocFetchMode::Rrf
        } else {
            AskExplainFullDocFetchMode::Cosine
        },
        final_source_order: inputs.final_source_order,
        context_char_budget: inputs.max_context_chars,
        context_chars_used: context.chars().count(),
        context_bytes_budget: inputs.max_context_chars,
        context_bytes_used: context.len(),
        rendered_context: None,
        truncated_by_budget,
    }
}

#[cfg(test)]
fn include_rendered_context(context: &str, explain_context: &mut AskExplainContext) {
    explain_context.rendered_context = Some(crate::services::types::AskExplainContextRendered {
        format: crate::services::types::AskExplainRenderedContextFormat::AxonSourcesV1,
        content: context.to_string(),
        bytes_used: context.len(),
        chars_used: context.chars().count(),
    });
}

fn renumber_source_header(entry: &str, display_id: usize) -> String {
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

#[cfg(test)]
mod rendered_context_tests {
    use super::*;
    use crate::vector::ops::ranking::AskCandidate;

    fn candidate(url: &str, text: &str, score: f64) -> AskCandidate {
        AskCandidate {
            score,
            url: url.to_string(),
            path: url.to_string(),
            chunk_text: text.to_string(),
            url_tokens: HashSet::new(),
            chunk_tokens: HashSet::new(),
            rerank_score: score,
        }
    }

    #[test]
    fn included_rendered_context_matches_final_renumbered_context() {
        let reranked = vec![
            candidate("https://docs.example.com/first", "first", 0.7),
            candidate("https://docs.example.com/second", "second", 0.9),
        ];
        let finalized = finalize_built_context(FinalizeContextInputs {
            reranked: &reranked,
            top_chunk_indices: &[0, 1],
            top_full_doc_indices: &[],
            selected_top_chunk_indices: &[0, 1],
            planned_full_doc_urls_set: &HashSet::new(),
            inserted_full_doc_urls: &HashSet::new(),
            supplemental: &[],
            supplemental_count: 0,
            top_chunks_selected: 2,
            full_docs_selected: 0,
            max_context_chars: 1_000,
            skip_decision: SkipDecision {
                skip: false,
                reason: "disabled",
            },
            is_rrf: false,
            separator: super::super::CONTEXT_SEPARATOR,
            context_started: std::time::Instant::now(),
            context_entries: vec![
                (
                    0.7,
                    "## Top Chunk [S10]: docs.example.com/first\n\nfirst body".to_string(),
                ),
                (
                    0.9,
                    "## Top Chunk [S2]: docs.example.com/second\n\nsecond body\n\n[Excerpt truncated to fit the context budget.]"
                        .to_string(),
                ),
            ],
        })
        .expect("context finalizes");

        let mut explain = finalized.explain_context;
        include_rendered_context(&finalized.context, &mut explain);
        let rendered = explain.rendered_context.expect("rendered context");

        assert_eq!(rendered.content, finalized.context);
        assert!(
            rendered
                .content
                .starts_with("Sources:\n## Top Chunk [S1]: docs.example.com/second")
        );
        assert!(
            rendered
                .content
                .contains("## Top Chunk [S2]: docs.example.com/first")
        );
        assert!(
            rendered
                .content
                .contains("[Excerpt truncated to fit the context budget.]")
        );
        assert_eq!(rendered.bytes_used, finalized.context.len());
        assert_eq!(rendered.chars_used, finalized.context.chars().count());
    }
}

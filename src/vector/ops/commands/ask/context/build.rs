mod appenders;
mod diagnostics;
mod fetchers;
mod selection;
mod trace;

use super::super::timing::{AskTiming, AskTimingSlot};
use super::heuristics::{SkipDecision, should_inject_supplemental, should_skip_full_doc_fetch};
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::services::types::AskExplainContext;
use crate::vector::ops::qdrant;
use crate::vector::ops::ranking;
use anyhow::{Result, anyhow};
use std::collections::HashSet;

pub(super) use appenders::{
    append_full_docs_to_context, append_supplemental_chunks, append_top_chunks_to_context,
};
pub(super) use diagnostics::build_diagnostic_sources;
#[cfg(test)]
pub(super) use fetchers::ask_doc_cache;
pub(super) use fetchers::fetch_full_docs;
pub(super) use selection::{
    SelectionPolicy, collect_supplemental_candidate_indices, planned_full_doc_urls,
    select_context_indices,
};
use selection::{dominant_retrieval_hosts, full_doc_selection_score};
pub(super) use trace::{
    CandidateSelectionMetadata, ContextCandidateSelection, ContextSelectionInputs,
    build_context_selection_decisions, candidate_selection_key, context_source_candidate_count,
    final_source_order_from_context, selected_top_chunk_indices, sorted_urls,
};

pub(super) struct BuiltAskContext {
    pub(super) context: String,
    pub(super) chunks_selected: usize,
    pub(super) full_docs_selected: usize,
    pub(super) supplemental_count: usize,
    pub(super) context_elapsed_ms: u128,
    pub(super) diagnostic_sources: Vec<String>,
    /// True when the adaptive skip gate elided full-doc fetch this request.
    /// (bd axon_rust-30y)
    pub(super) full_doc_fetch_skipped: bool,
    /// Static reason string from the skip gate; useful for diagnostics even
    /// when the gate did not fire ("disabled", "insufficient_urls", etc.).
    pub(super) full_doc_fetch_skip_reason: &'static str,
    #[allow(dead_code)]
    pub(super) explain_context: AskExplainContext,
    #[allow(dead_code)]
    pub(super) selection_decisions: Vec<ContextCandidateSelection>,
}

pub(super) async fn build_context_from_candidates(
    cfg: &Config,
    reranked: &[ranking::AskCandidate],
    top_chunk_indices: &[usize],
    top_full_doc_indices: &[usize],
    min_supplemental_score: Option<f64>,
    query_tokens: &[String],
    timing: &mut AskTiming,
) -> Result<BuiltAskContext> {
    let ask_tuning = cfg.ask_config();
    let max_context_chars = ask_tuning.ask_max_context_chars;
    let backfill_limit = ask_tuning.ask_backfill_chunks;
    let doc_fetch_concurrency = ask_tuning.ask_doc_fetch_concurrency;
    let doc_chunk_limit = ask_tuning.ask_doc_chunk_limit;
    let context_started = std::time::Instant::now();
    let mut context_entries: Vec<(f64, String)> = Vec::new();
    let mut context_char_count = 0usize;
    let separator = "\n\n---\n\n";
    let mut source_idx = 1usize;
    let is_rrf = min_supplemental_score.is_none();
    let skip_decision = should_skip_full_doc_fetch(cfg, reranked, is_rrf);
    let planned_full_doc_urls_set =
        planned_full_doc_urls(reranked, top_full_doc_indices, skip_decision.skip);
    let top_chunks_selected = append_top_chunks_to_context(
        reranked,
        top_chunk_indices,
        &planned_full_doc_urls_set,
        &mut context_entries,
        &mut context_char_count,
        &mut source_idx,
        separator,
        max_context_chars,
    );
    let selected_top_chunk_indices = selected_top_chunk_indices(
        reranked,
        top_chunk_indices,
        &planned_full_doc_urls_set,
        top_chunks_selected,
    );

    let mut inserted_full_doc_urls: HashSet<String> = HashSet::new();

    let fetched_docs = fetch_full_docs_for_context(FetchFullDocsInputs {
        cfg,
        reranked,
        top_full_doc_indices,
        context_char_count,
        max_context_chars,
        doc_chunk_limit,
        doc_fetch_concurrency,
        skip_decision,
        is_rrf,
        timing,
    })
    .await?;
    let (full_docs_selected, next_source_idx) = append_planned_full_docs(
        AppendPlannedFullDocsInputs {
            reranked,
            top_full_doc_indices,
            fetched_docs,
            query_tokens,
            separator,
            max_context_chars,
        },
        &mut context_entries,
        &mut context_char_count,
        &mut inserted_full_doc_urls,
        source_idx,
    );
    source_idx = next_source_idx;

    let supplemental_started = std::time::Instant::now();
    let (supplemental, supplemental_count) = maybe_inject_supplemental(
        reranked,
        &inserted_full_doc_urls,
        min_supplemental_score,
        full_docs_selected,
        top_chunks_selected,
        backfill_limit,
        max_context_chars,
        separator,
        &mut context_entries,
        &mut context_char_count,
        &mut source_idx,
    );
    timing.record(AskTimingSlot::Supplemental, supplemental_started);

    let finalized = finalize_built_context(FinalizeContextInputs {
        reranked,
        top_chunk_indices,
        top_full_doc_indices,
        selected_top_chunk_indices: &selected_top_chunk_indices,
        planned_full_doc_urls_set: &planned_full_doc_urls_set,
        inserted_full_doc_urls: &inserted_full_doc_urls,
        supplemental: &supplemental,
        supplemental_count,
        top_chunks_selected,
        full_docs_selected,
        max_context_chars,
        skip_decision,
        is_rrf,
        separator,
        context_started,
        context_entries,
    })?;

    Ok(BuiltAskContext {
        context: finalized.context,
        chunks_selected: top_chunks_selected,
        full_docs_selected,
        supplemental_count,
        context_elapsed_ms: finalized.context_elapsed_ms,
        diagnostic_sources: finalized.diagnostic_sources,
        full_doc_fetch_skipped: skip_decision.skip,
        full_doc_fetch_skip_reason: skip_decision.reason,
        explain_context: finalized.explain_context,
        selection_decisions: finalized.selection_decisions,
    })
}

type FetchedFullDocs = Vec<(usize, String, Vec<qdrant::QdrantPoint>)>;

struct AppendPlannedFullDocsInputs<'a> {
    reranked: &'a [ranking::AskCandidate],
    top_full_doc_indices: &'a [usize],
    fetched_docs: FetchedFullDocs,
    query_tokens: &'a [String],
    separator: &'a str,
    max_context_chars: usize,
}

fn append_planned_full_docs(
    inputs: AppendPlannedFullDocsInputs<'_>,
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    inserted_full_doc_urls: &mut HashSet<String>,
    source_idx: usize,
) -> (usize, usize) {
    // Map URL → entity-aware full-doc score for sort-by-score in the flattened
    // context. Raw top chunks can be broad matches; full docs should lead when
    // their path/title-like URL tokens match discriminating query entities.
    let dominant_hosts = dominant_retrieval_hosts(inputs.reranked);
    let url_to_score: std::collections::HashMap<String, f64> = inputs
        .top_full_doc_indices
        .iter()
        .filter_map(|&idx| {
            inputs.reranked.get(idx).map(|c| {
                (
                    c.url.clone(),
                    full_doc_selection_score(c, inputs.query_tokens, &dominant_hosts),
                )
            })
        })
        .collect();
    append_full_docs_to_context(
        context_entries,
        context_char_count,
        inserted_full_doc_urls,
        source_idx,
        inputs.separator,
        inputs.max_context_chars,
        inputs.fetched_docs,
        inputs.query_tokens,
        &url_to_score,
    )
}

struct FetchFullDocsInputs<'a> {
    cfg: &'a Config,
    reranked: &'a [ranking::AskCandidate],
    top_full_doc_indices: &'a [usize],
    context_char_count: usize,
    max_context_chars: usize,
    doc_chunk_limit: usize,
    doc_fetch_concurrency: usize,
    skip_decision: SkipDecision,
    is_rrf: bool,
    timing: &'a mut AskTiming,
}

async fn fetch_full_docs_for_context(inputs: FetchFullDocsInputs<'_>) -> Result<FetchedFullDocs> {
    let full_doc_fetch_started = std::time::Instant::now();
    let fetched_docs = if inputs.skip_decision.skip {
        log_info(&format!(
            "ask: skipping full-doc fetch (reason: {}; mode: {})",
            inputs.skip_decision.reason,
            if inputs.is_rrf { "rrf" } else { "cosine" }
        ));
        Vec::new()
    } else {
        fetch_full_docs(
            inputs.cfg,
            inputs.reranked,
            inputs.top_full_doc_indices,
            inputs.context_char_count,
            inputs.max_context_chars,
            inputs.doc_chunk_limit,
            inputs.doc_fetch_concurrency,
        )
        .await?
    };
    inputs
        .timing
        .record(AskTimingSlot::FullDocFetch, full_doc_fetch_started);
    Ok(fetched_docs)
}

struct FinalizeContextInputs<'a> {
    reranked: &'a [ranking::AskCandidate],
    top_chunk_indices: &'a [usize],
    top_full_doc_indices: &'a [usize],
    selected_top_chunk_indices: &'a [usize],
    planned_full_doc_urls_set: &'a HashSet<String>,
    inserted_full_doc_urls: &'a HashSet<String>,
    supplemental: &'a [usize],
    supplemental_count: usize,
    top_chunks_selected: usize,
    full_docs_selected: usize,
    max_context_chars: usize,
    skip_decision: SkipDecision,
    is_rrf: bool,
    separator: &'a str,
    context_started: std::time::Instant,
    context_entries: Vec<(f64, String)>,
}

struct FinalizedAskContext {
    context: String,
    context_elapsed_ms: u128,
    diagnostic_sources: Vec<String>,
    explain_context: AskExplainContext,
    selection_decisions: Vec<ContextCandidateSelection>,
}

fn finalize_built_context(mut inputs: FinalizeContextInputs<'_>) -> Result<FinalizedAskContext> {
    if inputs.context_entries.is_empty() {
        return Err(anyhow!("Failed to retrieve any context sources for ask"));
    }

    // Flatten by rerank_score across all buckets (top-chunks/full-docs/supplemental):
    // LLMs have proximity bias — highest-scoring chunks should appear first
    // regardless of which bucket they came from. (bd axon_rust-az9)
    inputs
        .context_entries
        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let joined = inputs
        .context_entries
        .iter()
        .enumerate()
        .map(|(idx, (_, entry))| renumber_source_header(entry, idx + 1))
        .collect::<Vec<_>>();
    let context = format!("Sources:\n{}", joined.join(inputs.separator));
    let explain_context = build_explain_context(
        &context,
        ExplainContextInputs {
            reranked: inputs.reranked,
            top_chunk_indices: inputs.top_chunk_indices,
            top_full_doc_indices: inputs.top_full_doc_indices,
            selected_top_chunk_indices: inputs.selected_top_chunk_indices,
            planned_full_doc_urls_set: inputs.planned_full_doc_urls_set,
            supplemental: inputs.supplemental,
            supplemental_count: inputs.supplemental_count,
            full_docs_selected: inputs.full_docs_selected,
            max_context_chars: inputs.max_context_chars,
            skip_decision: inputs.skip_decision,
            is_rrf: inputs.is_rrf,
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
            inputs.planned_full_doc_urls_set,
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
    supplemental: &'a [usize],
    supplemental_count: usize,
    full_docs_selected: usize,
    max_context_chars: usize,
    skip_decision: SkipDecision,
    is_rrf: bool,
}

fn build_explain_context(context: &str, inputs: ExplainContextInputs<'_>) -> AskExplainContext {
    let truncated_by_budget = inputs.selected_top_chunk_indices.len()
        + inputs.full_docs_selected
        + inputs.supplemental_count
        < context_source_candidate_count(
            inputs.reranked,
            inputs.top_chunk_indices,
            inputs.planned_full_doc_urls_set,
            inputs.top_full_doc_indices,
            inputs.supplemental,
            inputs.skip_decision.skip,
        );
    AskExplainContext {
        planned_full_doc_urls: sorted_urls(inputs.planned_full_doc_urls_set),
        full_doc_fetch_skipped: inputs.skip_decision.skip,
        full_doc_fetch_skip_reason: inputs.skip_decision.reason.to_string(),
        full_doc_fetch_mode: if inputs.is_rrf { "rrf" } else { "cosine" }.to_string(),
        final_source_order: final_source_order_from_context(context),
        context_char_budget: inputs.max_context_chars,
        context_chars_used: context.len(),
        truncated_by_budget,
    }
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

/// Run the supplemental backfill pass when coverage is thin and budget allows.
/// Extracted from `build_context_from_candidates` to keep that function under
/// the monolith policy's per-function line limit.
#[allow(clippy::too_many_arguments)]
fn maybe_inject_supplemental(
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

#[cfg(test)]
mod ask_doc_cache_tests {
    use super::ask_doc_cache;
    use crate::core::config::Config;

    #[test]
    fn ask_doc_cache_uses_runtime_cache_config() {
        let cfg = Config {
            ask_cache_max_capacity_bytes: 12_345,
            ask_cache_ttl_secs: 7,
            ..Config::default()
        };

        let cache = ask_doc_cache(&cfg);

        assert_eq!(cache.config().max_capacity_bytes, 12_345);
        assert_eq!(cache.config().effective_ttl_secs(), 7);
    }
}

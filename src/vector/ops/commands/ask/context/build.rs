mod appenders;
mod diagnostics;
mod fetchers;
mod logging;
mod selection;
mod supplemental;
mod trace;

use super::super::timing::{AskTiming, AskTimingSlot};
use super::heuristics::{SkipDecision, should_skip_full_doc_fetch};
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::services::types::AskExplainContext;
use crate::vector::ops::qdrant;
use crate::vector::ops::ranking;
use anyhow::{Result, anyhow};
use std::collections::HashSet;

pub(super) use appenders::{append_full_docs_to_context, append_top_chunks_to_context};
pub(super) use diagnostics::build_diagnostic_sources;
#[cfg(test)]
pub(super) use fetchers::ask_doc_cache;
pub(super) use fetchers::fetch_full_docs;
use logging::{ContextCompleteLog, ContextStartLog, log_context_complete, log_context_start};
#[cfg(test)]
pub(super) use selection::collect_supplemental_candidate_indices;
pub(super) use selection::{SelectionPolicy, planned_full_doc_urls, select_context_indices};
use selection::{dominant_retrieval_hosts, full_doc_selection_score};
use supplemental::maybe_inject_supplemental;
pub(super) use trace::{CandidateSelectionMetadata, candidate_selection_key};
pub(super) use trace::{
    ContextCandidateSelection, ContextSelectionInputs, build_context_selection_decisions,
    context_source_candidate_count, final_source_order_from_entries, selected_top_chunk_indices,
    sorted_urls,
};

const CONTEXT_SEPARATOR: &str = "\n\n---\n\n";

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
    let doc_fetch_concurrency = ask_tuning.ask_doc_fetch_concurrency;
    let doc_chunk_limit = ask_tuning.ask_doc_chunk_limit;
    let context_started = std::time::Instant::now();
    let mut context_entries: Vec<(f64, String)> = Vec::new();
    let mut context_char_count = 0usize;
    let mut source_idx = 1usize;
    let is_rrf = min_supplemental_score.is_none();
    let skip_decision = should_skip_full_doc_fetch(cfg, reranked, is_rrf);
    let planned_full_doc_urls_set =
        planned_full_doc_urls(reranked, top_full_doc_indices, skip_decision.skip);
    log_context_start(ContextStartLog {
        reranked_len: reranked.len(),
        top_chunks_len: top_chunk_indices.len(),
        top_full_docs_len: top_full_doc_indices.len(),
        max_context_chars,
        doc_chunk_limit,
        doc_fetch_concurrency,
        skip_full_docs: skip_decision.skip,
        skip_reason: skip_decision.reason,
    });
    let mut inserted_full_doc_urls: HashSet<String> = HashSet::new();

    // Full documents are inserted BEFORE top chunks so authoritative complete
    // pages get budget priority over individual chunks. (bd axon_rust-5map)
    let (full_docs_selected, next_source_idx) = append_full_docs_phase(
        FullDocsPhaseInputs {
            cfg,
            reranked,
            top_full_doc_indices,
            query_tokens,
            context_char_count,
            doc_chunk_limit,
            doc_fetch_concurrency,
            skip_decision,
            is_rrf,
            separator: CONTEXT_SEPARATOR,
            max_context_chars,
            timing,
        },
        &mut context_entries,
        &mut context_char_count,
        &mut inserted_full_doc_urls,
        source_idx,
    )
    .await?;
    source_idx = next_source_idx;

    let top_chunks_selected = append_top_chunks_phase(
        AppendTopChunksInputs {
            reranked,
            top_chunk_indices,
            planned_full_doc_urls_set: &planned_full_doc_urls_set,
            separator: CONTEXT_SEPARATOR,
            max_context_chars,
        },
        &mut context_entries,
        &mut context_char_count,
        &mut source_idx,
    );
    let selected_top_chunk_indices = selected_top_chunk_indices(
        reranked,
        top_chunk_indices,
        &planned_full_doc_urls_set,
        top_chunks_selected,
    );

    let supplemental_started = std::time::Instant::now();
    let (supplemental, supplemental_count) = maybe_inject_supplemental(
        reranked,
        &inserted_full_doc_urls,
        min_supplemental_score,
        full_docs_selected,
        top_chunks_selected,
        ask_tuning.ask_backfill_chunks,
        max_context_chars,
        CONTEXT_SEPARATOR,
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
        separator: CONTEXT_SEPARATOR,
        context_started,
        context_entries,
    })?;
    log_context_complete(ContextCompleteLog {
        top_chunks_selected,
        full_docs_selected,
        supplemental_count,
        context_chars: finalized.context.len(),
        elapsed_ms: finalized.context_elapsed_ms,
    });

    Ok(to_built_ask_context(ToBuiltContextInputs {
        finalized,
        top_chunks_selected,
        full_docs_selected,
        supplemental_count,
        skip_decision,
    }))
}

type FetchedFullDocs = Vec<(usize, String, Vec<qdrant::QdrantPoint>)>;

struct FullDocsPhaseInputs<'a> {
    cfg: &'a Config,
    reranked: &'a [ranking::AskCandidate],
    top_full_doc_indices: &'a [usize],
    query_tokens: &'a [String],
    context_char_count: usize,
    doc_chunk_limit: usize,
    doc_fetch_concurrency: usize,
    skip_decision: SkipDecision,
    is_rrf: bool,
    separator: &'a str,
    max_context_chars: usize,
    timing: &'a mut AskTiming,
}

async fn append_full_docs_phase(
    inputs: FullDocsPhaseInputs<'_>,
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    inserted_full_doc_urls: &mut HashSet<String>,
    source_idx: usize,
) -> Result<(usize, usize)> {
    let fetched_docs = fetch_full_docs_for_context(FetchFullDocsInputs {
        cfg: inputs.cfg,
        reranked: inputs.reranked,
        top_full_doc_indices: inputs.top_full_doc_indices,
        context_char_count: inputs.context_char_count,
        max_context_chars: inputs.max_context_chars,
        doc_chunk_limit: inputs.doc_chunk_limit,
        doc_fetch_concurrency: inputs.doc_fetch_concurrency,
        skip_decision: inputs.skip_decision,
        is_rrf: inputs.is_rrf,
        timing: inputs.timing,
    })
    .await?;
    Ok(append_planned_full_docs(
        AppendPlannedFullDocsInputs {
            reranked: inputs.reranked,
            top_full_doc_indices: inputs.top_full_doc_indices,
            fetched_docs,
            query_tokens: inputs.query_tokens,
            separator: inputs.separator,
            max_context_chars: inputs.max_context_chars,
        },
        context_entries,
        context_char_count,
        inserted_full_doc_urls,
        source_idx,
    ))
}

struct AppendTopChunksInputs<'a> {
    reranked: &'a [ranking::AskCandidate],
    top_chunk_indices: &'a [usize],
    planned_full_doc_urls_set: &'a HashSet<String>,
    separator: &'a str,
    max_context_chars: usize,
}

fn append_top_chunks_phase(
    inputs: AppendTopChunksInputs<'_>,
    context_entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    source_idx: &mut usize,
) -> usize {
    append_top_chunks_to_context(
        inputs.reranked,
        inputs.top_chunk_indices,
        inputs.planned_full_doc_urls_set,
        context_entries,
        context_char_count,
        source_idx,
        inputs.separator,
        inputs.max_context_chars,
    )
}

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
    log_info(&format!(
        "ask full-doc fetch complete planned={} fetched={} elapsed_ms={}",
        inputs.top_full_doc_indices.len(),
        fetched_docs.len(),
        full_doc_fetch_started.elapsed().as_millis(),
    ));
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
    let final_source_order = final_source_order_from_entries(&inputs.context_entries);
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
    final_source_order: Vec<crate::services::types::AskExplainContextSource>,
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
        final_source_order: inputs.final_source_order,
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

struct ToBuiltContextInputs {
    finalized: FinalizedAskContext,
    top_chunks_selected: usize,
    full_docs_selected: usize,
    supplemental_count: usize,
    skip_decision: SkipDecision,
}

fn to_built_ask_context(inputs: ToBuiltContextInputs) -> BuiltAskContext {
    BuiltAskContext {
        context: inputs.finalized.context,
        chunks_selected: inputs.top_chunks_selected,
        full_docs_selected: inputs.full_docs_selected,
        supplemental_count: inputs.supplemental_count,
        context_elapsed_ms: inputs.finalized.context_elapsed_ms,
        diagnostic_sources: inputs.finalized.diagnostic_sources,
        full_doc_fetch_skipped: inputs.skip_decision.skip,
        full_doc_fetch_skip_reason: inputs.skip_decision.reason,
        explain_context: inputs.finalized.explain_context,
        selection_decisions: inputs.finalized.selection_decisions,
    }
}

#[cfg(test)]
#[path = "build_ask_doc_cache_tests.rs"]
mod ask_doc_cache_tests;

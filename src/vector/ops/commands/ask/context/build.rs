mod appenders;
mod diagnostics;
mod fetchers;
mod selection;

use super::super::timing::{AskTiming, AskTimingSlot};
use super::heuristics::{
    push_context_entry, should_inject_supplemental, should_skip_full_doc_fetch,
};
use crate::core::config::Config;
use crate::core::logging::log_info;
use crate::vector::ops::ranking;
use anyhow::{Result, anyhow};
use std::collections::HashSet;

pub(super) use appenders::{
    append_full_docs_to_context, append_supplemental_chunks, append_top_chunks_to_context,
};
pub(super) use diagnostics::build_diagnostic_sources;
pub(super) use fetchers::{ask_doc_cache, fetch_full_docs};
pub(super) use selection::{
    collect_supplemental_candidate_indices, planned_full_doc_urls, select_context_indices,
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
    // Adaptive skip gate. `min_supplemental_score == None` is the canonical
    // signal that retrieval ran in RRF mode (see retrieval::is_rrf_mode +
    // AskRetrieval::min_supplemental_score). Use that to dispatch the gate's
    // mode-aware threshold without re-threading is_rrf through the call site.
    // (bd axon_rust-30y)
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

    let mut inserted_full_doc_urls: HashSet<String> = HashSet::new();

    let full_doc_fetch_started = std::time::Instant::now();
    let fetched_docs = if skip_decision.skip {
        log_info(&format!(
            "ask: skipping full-doc fetch (reason: {}; mode: {})",
            skip_decision.reason,
            if is_rrf { "rrf" } else { "cosine" }
        ));
        Vec::new()
    } else {
        fetch_full_docs(
            cfg,
            reranked,
            top_full_doc_indices,
            context_char_count,
            max_context_chars,
            doc_chunk_limit,
            doc_fetch_concurrency,
        )
        .await?
    };
    timing.record(AskTimingSlot::FullDocFetch, full_doc_fetch_started);
    // Map URL → rerank_score for sort-by-score in the flattened context.
    let url_to_score: std::collections::HashMap<String, f64> = top_full_doc_indices
        .iter()
        .filter_map(|&idx| reranked.get(idx).map(|c| (c.url.clone(), c.rerank_score)))
        .collect();
    let (full_docs_selected, next_source_idx) = append_full_docs_to_context(
        &mut context_entries,
        &mut context_char_count,
        &mut inserted_full_doc_urls,
        source_idx,
        separator,
        max_context_chars,
        fetched_docs,
        query_tokens,
        &url_to_score,
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

    if context_entries.is_empty() {
        return Err(anyhow!("Failed to retrieve any context sources for ask"));
    }

    // Flatten by rerank_score across all buckets (top-chunks/full-docs/supplemental):
    // LLMs have proximity bias — highest-scoring chunks should appear first
    // regardless of which bucket they came from. (bd axon_rust-az9)
    context_entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let joined = context_entries
        .into_iter()
        .enumerate()
        .map(|(idx, (_, entry))| renumber_source_header(&entry, idx + 1))
        .collect::<Vec<_>>();
    let context = format!("Sources:\n{}", joined.join(separator));
    let context_elapsed_ms = context_started.elapsed().as_millis();

    let diagnostic_sources = build_diagnostic_sources(
        reranked,
        top_chunk_indices,
        top_chunks_selected,
        &planned_full_doc_urls_set,
        top_full_doc_indices,
        &supplemental,
        supplemental_count,
    );

    Ok(BuiltAskContext {
        context,
        chunks_selected: top_chunks_selected,
        full_docs_selected,
        supplemental_count,
        context_elapsed_ms,
        diagnostic_sources,
        full_doc_fetch_skipped: skip_decision.skip,
        full_doc_fetch_skip_reason: skip_decision.reason,
    })
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

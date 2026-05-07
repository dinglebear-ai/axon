use super::super::timing::{AskTiming, AskTimingSlot};
use super::heuristics::{push_context_entry, should_inject_supplemental};
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::cache::{DocCacheKey, current_generation, global_doc_cache};
use crate::crates::vector::ops::source_display::display_source;
use crate::crates::vector::ops::{qdrant, ranking};
use anyhow::{Result, anyhow};
use futures_util::stream::{self, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;

pub(super) struct BuiltAskContext {
    pub(super) context: String,
    pub(super) chunks_selected: usize,
    pub(super) full_docs_selected: usize,
    pub(super) supplemental_count: usize,
    pub(super) context_elapsed_ms: u128,
    pub(super) diagnostic_sources: Vec<String>,
}

pub(super) fn select_context_indices(
    reranked: &[ranking::AskCandidate],
    chunk_limit: usize,
    full_doc_limit: usize,
) -> (Vec<usize>, Vec<usize>) {
    let top_chunk_indices = ranking::select_diverse_candidates(reranked, chunk_limit, 1);
    let chunk_urls = top_chunk_indices
        .iter()
        .filter_map(|&idx| reranked.get(idx).map(|candidate| candidate.url.as_str()))
        .collect::<HashSet<_>>();
    let full_doc_candidates = (0..reranked.len())
        .filter(|&idx| !chunk_urls.contains(reranked[idx].url.as_str()))
        .collect::<Vec<_>>();
    let top_full_doc_indices = ranking::select_diverse_candidates_from_indices(
        reranked,
        &full_doc_candidates,
        full_doc_limit,
        1,
    );
    (top_chunk_indices, top_full_doc_indices)
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
    let planned_full_doc_urls = top_full_doc_indices
        .iter()
        .filter_map(|&idx| reranked.get(idx).map(|candidate| candidate.url.clone()))
        .collect::<HashSet<_>>();
    let top_chunks_selected = append_top_chunks_to_context(
        reranked,
        top_chunk_indices,
        &planned_full_doc_urls,
        &mut context_entries,
        &mut context_char_count,
        &mut source_idx,
        separator,
        max_context_chars,
    );

    let mut inserted_full_doc_urls: HashSet<String> = HashSet::new();
    let full_doc_fetch_started = std::time::Instant::now();
    let fetched_docs = fetch_full_docs(
        cfg,
        reranked,
        top_full_doc_indices,
        context_char_count,
        max_context_chars,
        doc_chunk_limit,
        doc_fetch_concurrency,
    )
    .await?;
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

    let mut supplemental: Vec<usize> = Vec::new();
    let mut supplemental_count = 0usize;
    let supplemental_started = std::time::Instant::now();
    if should_inject_supplemental(
        context_char_count,
        max_context_chars,
        full_docs_selected,
        top_chunks_selected,
    ) {
        let supplemental_candidate_indices = collect_supplemental_candidate_indices(
            reranked,
            &inserted_full_doc_urls,
            min_supplemental_score,
        );
        supplemental = ranking::select_diverse_candidates_from_indices(
            reranked,
            &supplemental_candidate_indices,
            backfill_limit,
            1,
        );

        supplemental_count = append_supplemental_chunks(
            reranked,
            &supplemental,
            &mut context_entries,
            &mut context_char_count,
            &mut source_idx,
            separator,
            max_context_chars,
        );
    }
    timing.record(AskTimingSlot::Supplemental, supplemental_started);

    if context_entries.is_empty() {
        return Err(anyhow!("Failed to retrieve any context sources for ask"));
    }

    // Flatten by rerank_score across all buckets (top-chunks/full-docs/supplemental):
    // LLMs have proximity bias — highest-scoring chunks should appear first
    // regardless of which bucket they came from. (bd axon_rust-az9)
    context_entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let joined: Vec<String> = context_entries.into_iter().map(|(_, s)| s).collect();
    let context = format!("Sources:\n{}", joined.join(separator));
    let context_elapsed_ms = context_started.elapsed().as_millis();

    let diagnostic_sources = build_diagnostic_sources(
        reranked,
        top_chunk_indices,
        top_chunks_selected,
        &planned_full_doc_urls,
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
    })
}

#[allow(clippy::too_many_arguments)]
fn append_top_chunks_to_context(
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

pub(super) fn collect_supplemental_candidate_indices(
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

async fn fetch_full_docs(
    cfg: &Config,
    reranked: &[ranking::AskCandidate],
    top_full_doc_indices: &[usize],
    context_char_count: usize,
    max_context_chars: usize,
    doc_chunk_limit: usize,
    doc_fetch_concurrency: usize,
) -> Result<Vec<(usize, String, Vec<qdrant::QdrantPoint>)>> {
    let mut fetched_docs = Vec::new();
    if context_char_count >= max_context_chars {
        return Ok(fetched_docs);
    }
    let cfg_arc = Arc::new(cfg.clone());
    // Cache enable gate. The cache itself is process-global; we only consult
    // it when `cfg.ask_cache_enabled`. Snapshot the per-collection generation
    // once for this fetch batch so all concurrent lookups in this stream see
    // a consistent key. (axon_rust-pmc)
    let cache_enabled = cfg.ask_cache_enabled;
    let collection = cfg.collection.clone();
    let generation = if cache_enabled {
        current_generation(&collection)
    } else {
        0
    };
    // Collect owned `(order, doc_idx)` pairs before mapping to async tasks so
    // the map closure receives `(usize, usize)` (no lifetime-parameterised
    // `&usize`).  The reference pattern `|(order, &doc_idx)|` or even receiving
    // `(usize, &usize)` causes an HRTB `FnOnce` diagnostic when the resulting
    // future is verified for `Send + 'static` by `tokio::spawn`.
    let tasks: Vec<(usize, usize)> = top_full_doc_indices.iter().copied().enumerate().collect();
    let mut fetch_stream = stream::iter(tasks.into_iter().map(|(order, doc_idx)| {
        let cfg_for_task = Arc::clone(&cfg_arc);
        let url = reranked[doc_idx].url.clone();
        let collection = collection.clone();
        async move {
            let points = if cache_enabled {
                let key = DocCacheKey {
                    collection,
                    url: url.clone(),
                    generation,
                };
                let cfg_for_fetch = Arc::clone(&cfg_for_task);
                let url_for_fetch = url.clone();
                global_doc_cache()
                    .get_or_fetch(key, move || async move {
                        qdrant::qdrant_retrieve_by_url(
                            &cfg_for_fetch,
                            &url_for_fetch,
                            Some(doc_chunk_limit),
                        )
                        .await
                    })
                    .await
                    .map(|arc| (*arc).clone())
            } else {
                qdrant::qdrant_retrieve_by_url(&cfg_for_task, &url, Some(doc_chunk_limit)).await
            };
            (order, url, points)
        }
    }))
    .buffer_unordered(doc_fetch_concurrency);
    while let Some((order, url, points)) = fetch_stream.next().await {
        match points {
            Ok(points) => fetched_docs.push((order, url, points)),
            Err(err) => {
                log_warn(&format!(
                    "ask: failed to retrieve full document for {url}; continuing with remaining context: {err}"
                ));
            }
        }
    }
    fetched_docs.sort_by_key(|(order, _, _)| *order);
    Ok(fetched_docs)
}

/// Number of chunks per fetched full-doc that survive the query-relevance
/// filter before being concatenated. Tradeoff: small enough to drop irrelevant
/// chunks, large enough to preserve narrative context. (bd axon_rust-0fz)
const FULL_DOC_RENDER_TOP_K: usize = 24;

#[allow(clippy::too_many_arguments)]
fn append_full_docs_to_context(
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

fn append_supplemental_chunks(
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

fn build_diagnostic_sources(
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
            .map(|c| format!("chunk score={:.3} url={}", c.score, display_source(&c.url))),
    );
    diagnostic_sources.extend(
        top_full_doc_indices
            .iter()
            .map(|&idx| &reranked[idx])
            .map(|c| {
                format!(
                    "full-doc score={:.3} url={}",
                    c.score,
                    display_source(&c.url)
                )
            }),
    );
    diagnostic_sources.extend(
        supplemental
            .iter()
            .map(|&idx| &reranked[idx])
            .take(supplemental_count)
            .map(|c| format!("chunk score={:.3} url={}", c.score, display_source(&c.url))),
    );
    diagnostic_sources
}

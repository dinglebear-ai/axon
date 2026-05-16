use crate::core::config::Config;
use crate::core::logging::log_warn;
use anyhow::Result;

mod build;
mod heuristics;
mod query_rewrite;
mod retrieval;
#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;

use super::AskTiming;
use crate::services::types::{AskExplainTrace, CorpusHealthDiagnostic, CorpusHealthKind};
use build::build_context_from_candidates;
use query_rewrite::{QueryComplexity, build_query_forms};
use retrieval::retrieve_ask_candidates;
use spider::url::Url;

const ASK_EXPLAIN_CANDIDATE_TRACE_LIMIT: usize = 50;

/// Source of the resolved `ask_full_docs` value, surfaced in `ask` diagnostics
/// so operators can see whether the adaptive resolver fired or the user's
/// explicit override carried through. (bd axon_rust-721)
#[derive(Debug, Clone, Copy)]
pub(crate) enum FullDocsSource {
    UserOverride,
    AdaptiveSimple,
    AdaptiveComplex,
}

impl FullDocsSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FullDocsSource::UserOverride => "user_override",
            FullDocsSource::AdaptiveSimple => "adaptive_simple",
            FullDocsSource::AdaptiveComplex => "adaptive_complex",
        }
    }
}

/// Pure resolver for the `ask_full_docs` value: user override beats the
/// adaptive default driven by `QueryComplexity`. Extracted as a pure
/// function so the decision logic is unit-testable without the
/// retrieval / TEI / Qdrant stack. (bd axon_rust-721)
pub(crate) fn resolve_ask_full_docs(
    cfg_full_docs: usize,
    cfg_explicit: bool,
    complexity: QueryComplexity,
) -> (usize, FullDocsSource) {
    if cfg_explicit {
        (cfg_full_docs, FullDocsSource::UserOverride)
    } else {
        let value = complexity.full_docs_default();
        let source = match complexity {
            QueryComplexity::Simple => FullDocsSource::AdaptiveSimple,
            QueryComplexity::Complex => FullDocsSource::AdaptiveComplex,
        };
        (value, source)
    }
}

pub(crate) struct AskContext {
    pub context: String,
    pub graph_context_text: String,
    pub graph_entities_found: usize,
    pub candidate_count: usize,
    pub reranked_count: usize,
    pub chunks_selected: usize,
    pub full_docs_selected: usize,
    pub supplemental_count: usize,
    pub retrieval_elapsed_ms: u128,
    pub context_elapsed_ms: u128,
    pub graph_elapsed_ms: u128,
    pub diagnostic_sources: Vec<String>,
    pub top_domains: Vec<String>,
    pub authoritative_ratio: f64,
    pub configured_authority_ratio: f64,
    pub product_authority_ratio: f64,
    pub corpus_health: CorpusHealthDiagnostic,
    /// True when the adaptive skip gate elided full-doc fetch.
    /// (bd axon_rust-30y)
    pub full_doc_fetch_skipped: bool,
    /// Static reason string ("disabled", "ok_skip", "insufficient_urls", ...).
    pub full_doc_fetch_skip_reason: &'static str,
    /// Coarse query-complexity signal feeding the adaptive resolver below.
    /// "simple" or "complex". (bd axon_rust-721)
    pub detected_complexity: &'static str,
    /// Final `ask_full_docs` value used for this request after applying the
    /// adaptive resolver vs. user override. (bd axon_rust-721)
    pub resolved_full_docs: usize,
    /// "user_override" | "adaptive_simple" | "adaptive_complex".
    /// (bd axon_rust-721)
    pub full_docs_source: &'static str,
    pub explain: Option<AskExplainTrace>,
}

pub(crate) async fn build_ask_context(
    cfg: &Config,
    query: &str,
    timing: &mut AskTiming,
) -> Result<AskContext> {
    let retrieval = retrieve_ask_candidates(cfg, query, timing).await?;
    let query_tokens = crate::vector::ops::ranking::tokenize_query(query);

    // Adaptive `ask_full_docs` per query complexity. Single classifier
    // (`AskQueryForms.use_dual` → `QueryComplexity`) drives both the
    // existing dual-embedding decision and this resolution. retrieval.rs
    // already over-selected up to `cfg.ask_full_docs` indices, so we
    // narrow the slice down here without re-running selection.
    // (bd axon_rust-721)
    let query_forms = build_query_forms(query);
    let (resolved_full_docs, full_docs_source) = resolve_ask_full_docs(
        cfg.ask_full_docs,
        cfg.ask_full_docs_explicit,
        query_forms.complexity_hint,
    );
    let detected_complexity = match query_forms.complexity_hint {
        QueryComplexity::Simple => "simple",
        QueryComplexity::Complex => "complex",
    };

    let trim_to = resolved_full_docs.min(retrieval.top_full_doc_indices.len());
    let trimmed_full_doc_indices: Vec<usize> = retrieval.top_full_doc_indices[..trim_to].to_vec();

    let built = build_context_from_candidates(
        cfg,
        &retrieval.reranked,
        &retrieval.top_chunk_indices,
        &trimmed_full_doc_indices,
        retrieval.min_supplemental_score,
        &query_tokens,
        timing,
    )
    .await?;

    let graph_context_text = String::new();
    let graph_entities_found = 0usize;
    let graph_elapsed_ms = 0u128;
    let context = built.context;
    let selected_urls = selected_context_urls(&built.selection_decisions);
    let corpus_health = classify_corpus_health(
        &retrieval.top_domains,
        &selected_urls,
        retrieval.candidates.len(),
        context.len(),
    );

    if cfg.ask_graph {
        log_warn(
            "ask: --graph flag set but graph feature is not available in this build; using vector-only retrieval",
        );
    }

    Ok(AskContext {
        context,
        graph_context_text,
        graph_entities_found,
        candidate_count: retrieval.candidates.len(),
        reranked_count: retrieval.reranked.len(),
        chunks_selected: built.chunks_selected,
        full_docs_selected: built.full_docs_selected,
        supplemental_count: built.supplemental_count,
        retrieval_elapsed_ms: retrieval.retrieval_elapsed_ms,
        context_elapsed_ms: built.context_elapsed_ms,
        graph_elapsed_ms,
        diagnostic_sources: built.diagnostic_sources,
        top_domains: retrieval.top_domains,
        authoritative_ratio: retrieval.authoritative_ratio,
        configured_authority_ratio: retrieval.configured_authority_ratio,
        product_authority_ratio: retrieval.product_authority_ratio,
        corpus_health,
        full_doc_fetch_skipped: built.full_doc_fetch_skipped,
        full_doc_fetch_skip_reason: built.full_doc_fetch_skip_reason,
        detected_complexity,
        resolved_full_docs,
        full_docs_source: full_docs_source.as_str(),
        explain: if cfg.ask_explain {
            build_explain_trace(
                query,
                &retrieval.reranked,
                retrieval.explain_retrieval,
                retrieval.candidate_traces,
                built.explain_context,
                built.selection_decisions,
            )
        } else {
            None
        },
    })
}

fn build_explain_trace(
    query: &str,
    reranked: &[crate::vector::ops::ranking::AskCandidate],
    retrieval: Option<crate::services::types::AskExplainRetrieval>,
    candidate_traces: Vec<crate::vector::ops::commands::retrieval::CandidateRankingTrace>,
    context: crate::services::types::AskExplainContext,
    selections: Vec<build::ContextCandidateSelection>,
) -> Option<AskExplainTrace> {
    use crate::services::types::{AskExplainCandidate, AskExplainMode};
    use crate::vector::ops::ranking;
    use std::collections::{HashMap, VecDeque};

    let retrieval = retrieval?;
    let mut selections_by_key: HashMap<_, VecDeque<_>> = HashMap::new();
    for selection in selections {
        selections_by_key
            .entry(selection.key)
            .or_default()
            .push_back((selection.decisions, selection.metadata));
    }
    let mut raw_rerank_ranks: HashMap<_, VecDeque<_>> = HashMap::new();
    for (idx, candidate) in reranked.iter().enumerate() {
        raw_rerank_ranks
            .entry(build::candidate_selection_key(candidate))
            .or_default()
            .push_back(idx + 1);
    }
    let query_tokens = ranking::tokenize_query(query);
    let total_candidate_traces = candidate_traces.len();
    let candidates = candidate_traces
        .into_iter()
        .take(ASK_EXPLAIN_CANDIDATE_TRACE_LIMIT)
        .enumerate()
        .map(|(idx, trace)| {
            let (selection_decisions, selection_metadata) =
                if trace.filter_decisions.iter().any(|decision| {
                    decision.kind == crate::services::types::AskExplainFilterDecisionKind::Kept
                }) {
                    pop_front_for_key(
                        &mut selections_by_key,
                        &build::candidate_selection_key(&trace.candidate.candidate),
                    )
                    .unwrap_or_else(default_not_selected_selection)
                } else {
                    default_not_selected_selection()
                };
            let candidate = trace.candidate.candidate;
            let snippet = ranking::get_meaningful_snippet(&candidate.chunk_text, &query_tokens);
            let candidate_key = build::candidate_selection_key(&candidate);
            AskExplainCandidate {
                id: format!("candidate-{}", idx + 1),
                url: candidate.url,
                chunk_index: trace.candidate.chunk_index,
                raw_rerank_rank: pop_front_for_key(&mut raw_rerank_ranks, &candidate_key),
                planned_full_doc_rank: selection_metadata.planned_full_doc_rank,
                selected_context_rank: selection_metadata.selected_context_rank,
                insertion_mode: selection_metadata.insertion_mode,
                retrieval_score: candidate.score,
                rerank_score: candidate.rerank_score,
                score_kind: trace.score_kind,
                score_components: trace.score_components,
                filter_decisions: trace.filter_decisions,
                selection_decisions,
                snippet,
            }
        })
        .collect::<Vec<_>>();
    Some(AskExplainTrace {
        mode: AskExplainMode::ExplainOnly,
        retrieval,
        candidate_trace_limit: ASK_EXPLAIN_CANDIDATE_TRACE_LIMIT,
        candidate_trace_truncated: total_candidate_traces > ASK_EXPLAIN_CANDIDATE_TRACE_LIMIT,
        context,
        candidates,
        llm_skipped: true,
    })
}

fn pop_front_for_key<K, V>(
    values_by_key: &mut std::collections::HashMap<K, std::collections::VecDeque<V>>,
    key: &K,
) -> Option<V>
where
    K: Eq + std::hash::Hash,
{
    let values = values_by_key.get_mut(key)?;
    let value = values.pop_front();
    if values.is_empty() {
        values_by_key.remove(key);
    }
    value
}

fn selected_context_urls(selections: &[build::ContextCandidateSelection]) -> Vec<String> {
    selections
        .iter()
        .filter(|selection| {
            matches!(
                selection.metadata.insertion_mode,
                Some(
                    crate::services::types::AskExplainInsertionMode::TopChunk
                        | crate::services::types::AskExplainInsertionMode::InsertedFullDoc
                        | crate::services::types::AskExplainInsertionMode::Supplemental
                )
            )
        })
        .map(|selection| selection.url.clone())
        .collect()
}

fn classify_corpus_health(
    top_domains: &[String],
    selected_urls: &[String],
    candidate_pool: usize,
    context_chars: usize,
) -> CorpusHealthDiagnostic {
    let top_domain_count = top_domains.len();
    let selected_domain_count = selected_urls
        .iter()
        .filter_map(|url| Url::parse(url).ok())
        .filter_map(|url| url.host_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let (kind, reason) = if candidate_pool == 0 {
        (
            CorpusHealthKind::NoRetrievalCandidates,
            "retrieval returned no candidates".to_string(),
        )
    } else if selected_urls.is_empty() {
        (
            CorpusHealthKind::RetrievedNotSelected,
            "retrieval returned candidates but none reached selected context".to_string(),
        )
    } else if context_chars < 2_000 {
        (
            CorpusHealthKind::ThinDomain,
            "selected context is very small; indexed coverage may be thin".to_string(),
        )
    } else if top_domain_count == 0 {
        (
            CorpusHealthKind::Unknown,
            "top-domain diagnostics were unavailable".to_string(),
        )
    } else {
        (
            CorpusHealthKind::Healthy,
            "retrieval produced selected context".to_string(),
        )
    };

    CorpusHealthDiagnostic {
        kind,
        reason,
        selected_domain_count,
        top_domain_count,
    }
}

fn default_not_selected_selection() -> (
    Vec<crate::services::types::AskExplainSelectionDecision>,
    build::CandidateSelectionMetadata,
) {
    (
        vec![crate::services::types::AskExplainSelectionDecision {
            kind: crate::services::types::AskExplainSelectionDecisionKind::NotSelected,
            reason: None,
        }],
        build::CandidateSelectionMetadata {
            planned_full_doc_rank: None,
            selected_context_rank: None,
            insertion_mode: Some(crate::services::types::AskExplainInsertionMode::NotSelected),
        },
    )
}

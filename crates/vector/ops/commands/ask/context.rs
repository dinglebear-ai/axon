use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use anyhow::Result;

mod build;
mod heuristics;
mod query_rewrite;
mod retrieval;
#[cfg(test)]
mod tests;

use super::AskTiming;
use build::build_context_from_candidates;
use query_rewrite::{QueryComplexity, build_query_forms};
use retrieval::retrieve_ask_candidates;

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
}

pub(crate) async fn build_ask_context(
    cfg: &Config,
    query: &str,
    timing: &mut AskTiming,
) -> Result<AskContext> {
    let retrieval = retrieve_ask_candidates(cfg, query, timing).await?;
    let query_tokens = crate::crates::vector::ops::ranking::tokenize_query(query);

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
        full_doc_fetch_skipped: built.full_doc_fetch_skipped,
        full_doc_fetch_skip_reason: built.full_doc_fetch_skip_reason,
        detected_complexity,
        resolved_full_docs,
        full_docs_source: full_docs_source.as_str(),
    })
}

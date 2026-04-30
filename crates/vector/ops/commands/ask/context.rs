use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use anyhow::Result;

mod build;
mod heuristics;
mod retrieval;
#[cfg(test)]
mod tests;

use build::build_context_from_candidates;
use retrieval::retrieve_ask_candidates;

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
    pub dropped_by_allowlist: usize,
}

pub(crate) async fn build_ask_context(cfg: &Config, query: &str) -> Result<AskContext> {
    let retrieval = retrieve_ask_candidates(cfg, query).await?;
    let query_tokens = crate::crates::vector::ops::ranking::tokenize_query(query);
    let built = build_context_from_candidates(
        cfg,
        &retrieval.reranked,
        &retrieval.top_chunk_indices,
        &retrieval.top_full_doc_indices,
        &query_tokens,
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
        dropped_by_allowlist: retrieval.dropped_by_allowlist,
    })
}

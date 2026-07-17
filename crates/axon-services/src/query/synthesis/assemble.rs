//! Final [`AskResult`] assembly, ported from legacy
//! `axon_vector::ops::commands::ask::assemble`.
//!
//! [`assemble_explain_result`] is the `ask --explain` counterpart to
//! [`assemble_ask_result`]: issue #298's finale retires the legacy reranker
//! (and the `axon-vector` crate), so `ask --explain` now runs the SAME
//! retrieval-engine `AskContext` as a normal ask, skips the LLM call
//! entirely, and attaches an `AskExplainTrace` built from the retrieval hits
//! (`super::super::ask_retrieval::explain::build_explain_trace`) instead.
//! This mirrors the legacy `build_explain_result`'s no-synthesis shape
//! (empty `answer`, no `citation_validation`, `timing_ms.llm == 0`).

use super::AskContext;
use super::normalize::summarize_citation_validation;
use super::timing::AskTiming;
use axon_api::{
    AskCitationValidation, AskDiagnostics, AskExplainTrace, AskResult, AskTiming as WireAskTiming,
};
use axon_core::config::Config;
use axon_core::logging::log_info;

/// Build the final typed result for a completed ask.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble_ask_result(
    cfg: &Config,
    query: &str,
    ctx: &AskContext,
    answer: &str,
    llm_total_ms: u128,
    total_elapsed_ms: u128,
    timing: &AskTiming,
    diagnostics_enabled: bool,
) -> AskResult {
    log_info(&format!(
        "ask complete answer_chars={} llm_ms={} total_ms={}",
        answer.len(),
        llm_total_ms,
        total_elapsed_ms,
    ));
    let validation = summarize_citation_validation(answer);
    AskResult {
        query: query.to_string(),
        answer: answer.to_string(),
        citations: ctx.citations.clone(),
        citation_validation: Some(AskCitationValidation {
            valid: validation.valid,
            issues: validation.issues,
            canonical_citation_count: validation.canonical_citation_count,
        }),
        session: None,
        warnings: ctx.warnings.clone(),
        diagnostics: build_diagnostics(diagnostics_enabled, cfg, ctx),
        explain: None,
        timing_ms: build_timing(
            ctx.retrieval_elapsed_ms,
            ctx.context_elapsed_ms,
            llm_total_ms,
            total_elapsed_ms,
            timing,
        ),
    }
}

/// Build the final typed result for an `ask --explain` request.
///
/// Never calls the LLM: `answer` is empty, `citation_validation` is omitted
/// (there is no answer to validate citations against), diagnostics are always
/// populated (explain mode implies `diagnostics_enabled = true`, matching
/// `ask_result_from_context`'s `cfg.ask_diagnostics || cfg.ask_explain`
/// gate), and `explain` carries the caller-built trace. `timing_ms.llm` is
/// `0` and every LLM/streaming sub-stage field is `None`.
pub(crate) fn assemble_explain_result(
    cfg: &Config,
    query: &str,
    ctx: &AskContext,
    trace: AskExplainTrace,
    total_elapsed_ms: u128,
) -> AskResult {
    log_info(&format!(
        "ask explain complete total_ms={total_elapsed_ms} context_chars={}",
        ctx.context.len()
    ));
    AskResult {
        query: query.to_string(),
        answer: String::new(),
        citations: ctx.citations.clone(),
        citation_validation: None,
        session: None,
        warnings: ctx.warnings.clone(),
        diagnostics: build_diagnostics(true, cfg, ctx),
        explain: Some(trace),
        timing_ms: WireAskTiming {
            retrieval: ctx.retrieval_elapsed_ms,
            context_build: ctx.context_elapsed_ms,
            llm: 0,
            total: total_elapsed_ms,
            tei_embed_ms: None,
            qdrant_primary_ms: None,
            qdrant_secondary_ms: None,
            rerank_ms: None,
            top_select_ms: None,
            full_doc_fetch_ms: None,
            supplemental_ms: None,
            llm_ttft_ms: None,
            llm_total_ms: None,
            streamed: None,
            normalize_ms: None,
        },
    }
}

fn build_diagnostics(enabled: bool, cfg: &Config, ctx: &AskContext) -> Option<AskDiagnostics> {
    if !enabled {
        return None;
    }
    Some(AskDiagnostics {
        candidate_pool: ctx.candidate_count,
        reranked_pool: ctx.reranked_count,
        chunks_selected: ctx.chunks_selected,
        full_docs_selected: ctx.full_docs_selected,
        supplemental_selected: ctx.supplemental_count,
        context_chars: ctx.context.len(),
        full_doc_fetch_skipped: ctx.full_doc_fetch_skipped,
        full_doc_fetch_skip_reason: ctx.full_doc_fetch_skip_reason.to_string(),
        full_doc_fetch_errors: ctx.full_doc_fetch_errors.clone(),
        detected_complexity: ctx.detected_complexity.to_string(),
        resolved_full_docs: ctx.resolved_full_docs,
        full_docs_source: ctx.full_docs_source.to_string(),
        min_relevance_score: cfg.ask_min_relevance_score,
        ask_candidate_limit: cfg.ask_candidate_limit,
        ask_chunk_limit: cfg.ask_chunk_limit,
        ask_backfill_chunks: cfg.ask_backfill_chunks,
        ask_doc_chunk_limit: cfg.ask_doc_chunk_limit,
        ask_hybrid_candidates: cfg.ask_hybrid_candidates,
        ask_full_docs_configured: cfg.ask_full_docs,
        ask_full_docs_explicit: cfg.ask_full_docs_explicit,
        ask_fulldoc_skip_enabled: cfg.ask_fulldoc_skip_enabled,
        ask_max_context_chars: cfg.ask_max_context_chars,
        doc_fetch_concurrency: cfg.ask_doc_fetch_concurrency,
        top_domains: ctx.top_domains.clone(),
        authority_ratio: ctx.authoritative_ratio,
        configured_authority_ratio: ctx.configured_authority_ratio,
        product_authority_ratio: ctx.product_authority_ratio,
        corpus_health: Some(ctx.corpus_health.clone()),
    })
}

/// Back-compat: legacy timing shape always present; sub-stage fields populate
/// only when `cfg.ask_diagnostics` is true. Mirrors the historical
/// `build_timing_json` field-emission logic exactly — sub-stage / `streamed` /
/// `llm_ttft_ms` fields stay `None` unless the corresponding slot is set, and
/// `WireAskTiming`'s `skip_serializing_if = "Option::is_none"` reproduces the
/// previous JSON shape byte-for-byte.
fn build_timing(
    retrieval_ms: u128,
    context_ms: u128,
    llm_ms: u128,
    total_ms: u128,
    timing: &AskTiming,
) -> WireAskTiming {
    let mut out = WireAskTiming {
        retrieval: retrieval_ms,
        context_build: context_ms,
        llm: llm_ms,
        total: total_ms,
        tei_embed_ms: None,
        qdrant_primary_ms: None,
        qdrant_secondary_ms: None,
        rerank_ms: None,
        top_select_ms: None,
        full_doc_fetch_ms: None,
        supplemental_ms: None,
        llm_ttft_ms: None,
        llm_total_ms: None,
        streamed: None,
        normalize_ms: None,
    };

    // Without diagnostics: only streamed + ttft are ever emitted.
    if let AskTiming::Disabled {
        streamed,
        llm_ttft_ms,
        ..
    } = timing
    {
        out.streamed = *streamed;
        out.llm_ttft_ms = *llm_ttft_ms;
        return out;
    }
    let Some(e) = timing.enabled() else {
        return out;
    };
    out.tei_embed_ms = e.tei_embed_ms;
    out.qdrant_primary_ms = e.qdrant_primary_ms;
    out.qdrant_secondary_ms = e.qdrant_secondary_ms;
    out.rerank_ms = e.rerank_ms;
    out.top_select_ms = e.top_select_ms;
    out.full_doc_fetch_ms = e.full_doc_fetch_ms;
    out.supplemental_ms = e.supplemental_ms;
    out.llm_ttft_ms = e.llm_ttft_ms;
    out.llm_total_ms = e.llm_total_ms;
    out.normalize_ms = e.normalize_ms;
    out.streamed = e.streamed;
    out
}

#[cfg(test)]
#[path = "assemble_tests.rs"]
mod tests;

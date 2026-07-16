//! `evaluate`: RAG vs baseline + independent LLM judge, ported off legacy
//! `axon_vector::ops::commands::evaluate` (issue #298 cutover).
//!
//! The RAG-retrieval half (the `ask_ctx` parameter) is already built by the
//! caller (`crate::query::evaluate`) through the `axon-retrieval` engine via
//! [`super::ask_retrieval::retrieval_ask_context`] — mirroring the `ask`
//! cutover. This module owns the rest: the baseline answer, the judge (LLM
//! analysis + its own independent judge-reference retrieval), and synthesis,
//! all via [`super::synthesis`]. The judge-reference retrieval and the
//! `--retrieval-ab` dense-only comparison arm both now go through
//! `retrieval_ask_context` too (replacing legacy's `build_ask_context` calls),
//! so `evaluate` no longer needs the legacy reranker at all.
//!
//! **Disclosed gap:** `--retrieval-ab` mode compares a "hybrid" RAG answer
//! against a "dense-only" one by cloning `cfg` with `hybrid_search_enabled =
//! false` before the second retrieval call. The `axon-retrieval` engine does
//! not read `cfg.hybrid_search_enabled` (it was never wired into `run_query`/
//! `RetrievalEngine`, even before this port), so today both arms retrieve
//! identically and the "baseline" answer is not actually dense-only. This is a
//! pre-existing gap in the #298 cutover (not introduced by this port) —
//! `--retrieval-ab` will need `axon-retrieval` to grow a hybrid on/off knob
//! before it does anything again.

mod scoring;
mod streaming;

use axon_api::{
    EvaluateCrawlEnqueueOutcome, EvaluateDiagnostics, EvaluateResult, EvaluateTiming, Suggestion,
};
use axon_core::config::Config;
use axon_core::http::http_client;
use axon_core::logging::log_warn;
use std::time::Instant;

use crate::context::ServiceContext;
use crate::query::ask_retrieval::retrieval_ask_context;
use crate::query::suggest::discover_crawl_suggestions;
use crate::query::synthesis::AskContext;
use crate::query::synthesis::completion::JudgeContext;
use crate::query::synthesis::normalize::normalize_ask_answer;
use crate::query::synthesis::validate_ask_llm_config;
use scoring::{
    build_judge_reference, build_suggestion_focus, extract_source_urls, format_rag_sources,
    rag_underperformed,
};
use streaming::{run_analysis, run_baseline_answer, run_rag_answer};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CrawlSuggestion {
    url: String,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CrawlEnqueueOutcome {
    url: String,
    job_id: Option<String>,
    error: Option<String>,
}

/// Run the evaluate pipeline with a RAG context supplied by the caller.
///
/// `ctx` supplies the read-plane runtime used for the judge-reference and
/// (`--retrieval-ab`) dense-only retrieval passes. Forces `json_output = true`
/// so the non-streaming path is used.
pub(crate) async fn evaluate_result_with_context(
    ctx: &ServiceContext,
    cfg: &Config,
    query: String,
    ask_ctx: AskContext,
) -> Result<EvaluateResult, String> {
    let mut derived = cfg.clone();
    derived.json_output = true;
    validate_ask_llm_config(&derived).map_err(|err| err.to_string())?;
    let eval_started = Instant::now();
    evaluate_from_context(ctx, derived, query, ask_ctx, eval_started).await
}

/// Shared tail of the evaluate pipeline: given a RAG context, produce the RAG
/// + baseline answers, judge them, and assemble the `EvaluateResult`.
async fn evaluate_from_context(
    ctx: &ServiceContext,
    derived: Config,
    query: String,
    ask_ctx: AskContext,
    eval_started: Instant,
) -> Result<EvaluateResult, String> {
    let client = http_client().map_err(|err| err.to_string())?;
    let (rag_answer, rag_elapsed_ms, baseline_answer, baseline_elapsed_ms) =
        acquire_rag_and_baseline_answers(ctx, &derived, client, &query, &ask_ctx.context).await?;
    let normalized_rag_answer =
        normalize_ask_answer(&derived, &query, &rag_answer, &ask_ctx.context);
    let research_started = Instant::now();
    let (judge_reference, ref_chunk_count) = build_judge_reference(ctx, &derived, &query)
        .await
        .unwrap_or_else(|e| {
            log_warn(&format!(
                "evaluate: judge reference retrieval failed (proceeding without grounding): {e}"
            ));
            ("No reference material available.".to_string(), 0)
        });
    let research_elapsed_ms = research_started.elapsed().as_millis();
    let rag_sources_list = format_rag_sources(&ask_ctx.diagnostic_sources);
    let ref_quality_note = if ref_chunk_count < 3 {
        "\u{26a0}\u{fe0f}  Reference material is limited — accuracy scores may be less reliable.\n\n"
    } else {
        ""
    };
    let source_count =
        ask_ctx.chunks_selected + ask_ctx.full_docs_selected + ask_ctx.supplemental_count;
    let context_chars = ask_ctx.context.len();
    let judge_ctx = JudgeContext {
        query: &query,
        rag_answer: &normalized_rag_answer,
        baseline_answer: &baseline_answer,
        reference_chunks: &judge_reference,
        rag_sources_list: &rag_sources_list,
        ref_quality_note,
        rag_elapsed_ms,
        baseline_elapsed_ms,
        source_count,
        context_chars,
        retrieval_ab: derived.evaluate_retrieval_ab,
    };
    let (analysis_answer, analysis_elapsed_ms) = run_analysis(&derived, client, &judge_ctx).await?;
    let crawl_suggestions = if rag_underperformed(&analysis_answer) {
        let focus = build_suggestion_focus(&query, &analysis_answer);
        discover_crawl_suggestions(&derived, &focus, 5)
            .await
            .unwrap_or_else(|e| {
                log_warn(&format!(
                    "evaluate: suggestion discovery failed after rag underperformance: {e}"
                ));
                Vec::new()
            })
            .into_iter()
            .map(|(url, reason)| CrawlSuggestion { url, reason })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let crawl_enqueue_outcomes: Vec<CrawlEnqueueOutcome> = Vec::new();
    let total_elapsed_ms = eval_started.elapsed().as_millis();
    let source_urls = extract_source_urls(&ask_ctx.diagnostic_sources);

    let diagnostics = derived.ask_diagnostics.then_some(EvaluateDiagnostics {
        candidate_pool: ask_ctx.candidate_count,
        reranked_pool: ask_ctx.reranked_count,
        chunks_selected: ask_ctx.chunks_selected,
        full_docs_selected: ask_ctx.full_docs_selected,
        supplemental_selected: ask_ctx.supplemental_count,
        context_chars,
        min_relevance_score: derived.ask_min_relevance_score,
        doc_fetch_concurrency: derived.ask_doc_fetch_concurrency,
    });

    Ok(EvaluateResult {
        query,
        rag_answer: normalized_rag_answer,
        baseline_answer,
        analysis_answer,
        citations: ask_ctx.citations.clone(),
        source_urls,
        crawl_suggestions: crawl_suggestions
            .into_iter()
            .map(|s| Suggestion {
                url: s.url,
                reason: s.reason,
            })
            .collect(),
        crawl_enqueue_outcomes: crawl_enqueue_outcomes
            .into_iter()
            .map(|o| EvaluateCrawlEnqueueOutcome {
                url: o.url,
                job_id: o.job_id,
                error: o.error,
            })
            .collect(),
        ref_chunk_count,
        diagnostics,
        timing_ms: EvaluateTiming {
            retrieval: ask_ctx.retrieval_elapsed_ms,
            context_build: ask_ctx.context_elapsed_ms,
            rag_llm: rag_elapsed_ms,
            baseline_llm: baseline_elapsed_ms,
            research_elapsed_ms,
            analysis_llm_ms: analysis_elapsed_ms,
            total: total_elapsed_ms,
        },
    })
}

/// Produce the RAG and baseline answers (with their elapsed-ms timings) for
/// the evaluate run. In retrieval-A/B mode the "baseline" is a second RAG run
/// with hybrid retrieval disabled (see the module-level disclosed-gap note),
/// so the judge compares hybrid-RAG vs dense-only-RAG; otherwise it is the
/// no-context baseline. Returns
/// `(rag_answer, rag_elapsed_ms, baseline_answer, baseline_elapsed_ms)`.
async fn acquire_rag_and_baseline_answers(
    ctx: &ServiceContext,
    derived: &Config,
    client: &reqwest::Client,
    query: &str,
    rag_context: &str,
) -> Result<(String, u128, String, u128), String> {
    let rag_future = run_rag_answer(derived, client, query, rag_context);
    if derived.evaluate_retrieval_ab {
        let mut dense_cfg = derived.clone();
        dense_cfg.hybrid_search_enabled = false;
        let dense_ctx = retrieval_ask_context(ctx, &dense_cfg, query, "evaluate_retrieval_ab")
            .await
            .map_err(|e| e.to_string())?;
        let dense_future = run_rag_answer(&dense_cfg, client, query, &dense_ctx.context);
        let (rag, dense) = tokio::try_join!(rag_future, dense_future)?;
        Ok((rag.0, rag.1, dense.0, dense.1))
    } else {
        let baseline_future = run_baseline_answer(derived, client, query);
        let (rag, baseline) = tokio::try_join!(rag_future, baseline_future)?;
        Ok((rag.0, rag.1, baseline.0, baseline.1))
    }
}

// No dedicated test sidecar for this file: `evaluate_result_with_context`/
// `evaluate_from_context`/`acquire_rag_and_baseline_answers` require live
// LLM/Qdrant/TEI services to exercise meaningfully (same as the legacy
// crate, whose `evaluate_tests.rs` covered only `display`/`scoring`/
// `evaluate_query` — none of which this port carries: `display` was dead
// code, `evaluate_query` doesn't exist here since `axon-services::query::
// evaluate` already receives `question` directly). Pure-logic coverage lives
// in `evaluate::scoring_tests` and `synthesis::normalize_tests`.

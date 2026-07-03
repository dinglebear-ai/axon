mod display;
mod scoring;
mod streaming;

use axon_api::{
    EvaluateCrawlEnqueueOutcome, EvaluateDiagnostics, EvaluateResult, EvaluateTiming, Suggestion,
};
use axon_core::config::Config;
use axon_core::http::http_client;
use axon_core::logging::log_warn;
use std::time::Instant;

use super::ask::{AskContext, build_ask_context, normalize_ask_answer};
use super::suggest::discover_crawl_suggestions;
use scoring::{
    build_judge_reference, build_suggestion_focus, extract_source_urls, format_rag_sources,
    rag_underperformed,
};
use streaming::{run_analysis, run_baseline_answer, run_rag_answer};

// Used by the `streaming` submodule — clippy's dead_code lint does not cross
// module boundaries, so allow the lint here even though the type is live.
#[derive(Debug, Default)]
#[allow(dead_code)]
struct SideBySideBuffer {
    with_context: String,
    without_context: String,
}

#[allow(dead_code)]
impl SideBySideBuffer {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, stream: &str, delta: &str) {
        match stream {
            streaming::STREAM_WITH_CONTEXT => self.with_context.push_str(delta),
            streaming::STREAM_WITHOUT_CONTEXT => self.without_context.push_str(delta),
            _ => {
                log_warn(&format!(
                    "evaluate: SideBySideBuffer received unknown stream '{}' — delta discarded ({} chars)",
                    stream,
                    delta.len()
                ));
            }
        }
    }
}

struct EvalTiming {
    rag_elapsed_ms: u128,
    baseline_elapsed_ms: u128,
    research_elapsed_ms: u128,
    analysis_elapsed_ms: u128,
    total_elapsed_ms: u128,
}

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

struct EvalAnswers<'a> {
    rag: &'a str,
    baseline: &'a str,
    analysis: &'a str,
    crawl_suggestions: &'a [CrawlSuggestion],
    crawl_enqueue_outcomes: &'a [CrawlEnqueueOutcome],
    ref_chunk_count: usize,
    context_chars: usize,
}

/// Thin JSON wrapper preserved for compatibility / tests. Serializes the typed
/// `evaluate_result()` once; the services layer should call `evaluate_result()`
/// directly to avoid the serialize→deserialize round-trip.
///
/// Note: the JSON additionally carries a `scores` field (derived from the
/// analysis answer) that `EvaluateResult` does not model; it is regenerated here
/// so the wrapper stays byte-identical to the historical payload.
pub async fn evaluate_payload(cfg: &Config) -> Result<serde_json::Value, String> {
    let result = evaluate_result(cfg).await?;
    let mut value = serde_json::to_value(&result).map_err(|e| e.to_string())?;
    if let serde_json::Value::Object(map) = &mut value {
        map.insert(
            "scores".to_string(),
            serde_json::to_value(scoring::structured_scores_from_analysis(
                &result.analysis_answer,
            ))
            .map_err(|e| e.to_string())?,
        );
    }
    Ok(value)
}

/// Run the evaluate pipeline and return the typed `EvaluateResult` directly.
///
/// Forces `json_output = true` internally so the non-streaming path is used.
///
/// This entry point builds the RAG retrieval context on the LEGACY
/// `axon_vector` reranker path. The issue #298 cutover routes production
/// callers (CLI/MCP/REST via `axon-services`) through
/// [`evaluate_result_with_context`] instead, supplying a context built by the
/// `axon-retrieval` engine. This variant is retained for tests and any caller
/// that has no read-plane runtime.
pub async fn evaluate_result(cfg: &Config) -> Result<EvaluateResult, String> {
    let mut derived = cfg.clone();
    derived.json_output = true;
    let query = evaluate_query(&derived)?;
    let eval_started = Instant::now();
    let ctx = build_evaluate_ask_context(&derived, &query).await?;
    evaluate_from_context(derived, query, ctx, eval_started).await
}

/// Run the evaluate pipeline with a RAG context supplied by the caller.
///
/// Issue #298 cutover (this slice): the SEARCH + CONTEXT half of `evaluate` is
/// built by `axon-services` through the `axon-retrieval` engine
/// (`AskContext::from_retrieval`) and injected here, exactly mirroring the `ask`
/// cutover (PR #348). The baseline answer, the judge (LLM analysis + its
/// judge-reference retrieval), and all synthesis stay on the existing
/// `axon_vector`/core-llm path. Forces `json_output = true` so the non-streaming
/// path is used.
pub async fn evaluate_result_with_context(
    cfg: &Config,
    query: String,
    ctx: AskContext,
) -> Result<EvaluateResult, String> {
    let mut derived = cfg.clone();
    derived.json_output = true;
    super::ask::validate_ask_llm_config(&derived).map_err(|err| err.to_string())?;
    let eval_started = Instant::now();
    evaluate_from_context(derived, query, ctx, eval_started).await
}

/// Shared tail of the evaluate pipeline: given a RAG context (from either the
/// legacy reranker or the `axon-retrieval` engine), produce the RAG + baseline
/// answers, judge them, and assemble the `EvaluateResult`.
async fn evaluate_from_context(
    derived: Config,
    query: String,
    ctx: AskContext,
    eval_started: Instant,
) -> Result<EvaluateResult, String> {
    let client = http_client().map_err(|err| err.to_string())?;
    let (rag_answer, rag_elapsed_ms, baseline_answer, baseline_elapsed_ms) =
        acquire_rag_and_baseline_answers(&derived, client, &query, &ctx.context).await?;
    let normalized_rag_answer = normalize_ask_answer(&derived, &query, &rag_answer, &ctx.context);
    let research_started = Instant::now();
    let (judge_reference, ref_chunk_count) = build_judge_reference(&derived, &query)
        .await
        .unwrap_or_else(|e| {
            log_warn(&format!(
                "evaluate: judge reference retrieval failed (proceeding without grounding): {e}"
            ));
            ("No reference material available.".to_string(), 0)
        });
    let research_elapsed_ms = research_started.elapsed().as_millis();
    let rag_sources_list = format_rag_sources(&ctx.diagnostic_sources);
    let ref_quality_note = if ref_chunk_count < 3 {
        "\u{26a0}\u{fe0f}  Reference material is limited — accuracy scores may be less reliable.\n\n"
    } else {
        ""
    };
    let source_count = ctx.chunks_selected + ctx.full_docs_selected + ctx.supplemental_count;
    let context_chars = ctx.context.len();
    let judge_ctx = super::streaming::JudgeContext {
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
    let source_urls = extract_source_urls(&ctx.diagnostic_sources);

    let diagnostics = derived.ask_diagnostics.then_some(EvaluateDiagnostics {
        candidate_pool: ctx.candidate_count,
        reranked_pool: ctx.reranked_count,
        chunks_selected: ctx.chunks_selected,
        full_docs_selected: ctx.full_docs_selected,
        supplemental_selected: ctx.supplemental_count,
        context_chars,
        min_relevance_score: derived.ask_min_relevance_score,
        doc_fetch_concurrency: derived.ask_doc_fetch_concurrency,
    });

    Ok(EvaluateResult {
        query,
        rag_answer: normalized_rag_answer,
        baseline_answer,
        analysis_answer,
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
            retrieval: ctx.retrieval_elapsed_ms,
            context_build: ctx.context_elapsed_ms,
            rag_llm: rag_elapsed_ms,
            baseline_llm: baseline_elapsed_ms,
            research_elapsed_ms,
            analysis_llm_ms: analysis_elapsed_ms,
            total: total_elapsed_ms,
        },
    })
}

/// Produce the RAG and baseline answers (with their elapsed-ms timings) for the
/// evaluate run. In retrieval-A/B mode the "baseline" is a second RAG run with
/// hybrid retrieval disabled, so the judge compares hybrid-RAG vs dense-only-RAG;
/// otherwise it is the no-context baseline. Returns
/// `(rag_answer, rag_elapsed_ms, baseline_answer, baseline_elapsed_ms)`.
async fn acquire_rag_and_baseline_answers(
    derived: &Config,
    client: &reqwest::Client,
    query: &str,
    rag_context: &str,
) -> Result<(String, u128, String, u128), String> {
    let rag_future = run_rag_answer(derived, client, query, rag_context);
    if derived.evaluate_retrieval_ab {
        let mut dense_cfg = derived.clone();
        dense_cfg.hybrid_search_enabled = false;
        let dense_ctx = build_evaluate_ask_context(&dense_cfg, query).await?;
        let dense_future = run_rag_answer(&dense_cfg, client, query, &dense_ctx.context);
        let (rag, dense) = tokio::try_join!(rag_future, dense_future)?;
        Ok((rag.0, rag.1, dense.0, dense.1))
    } else {
        let baseline_future = run_baseline_answer(derived, client, query);
        let (rag, baseline) = tokio::try_join!(rag_future, baseline_future)?;
        Ok((rag.0, rag.1, baseline.0, baseline.1))
    }
}

fn evaluate_query(cfg: &Config) -> Result<String, String> {
    super::ask::validate_ask_llm_config(cfg).map_err(|err| err.to_string())?;
    super::resolve_query_text(cfg).ok_or_else(|| "evaluate requires a question".to_string())
}

async fn build_evaluate_ask_context(cfg: &Config, query: &str) -> Result<AskContext, String> {
    let mut timing = disabled_ask_timing();
    build_ask_context(cfg, query, &mut timing)
        .await
        .map_err(|err| err.to_string())
}

/// `evaluate` uses its own `EvaluateTiming` shape; ask sub-stage timings are
/// not surfaced from the evaluate path, so this helper produces a disabled
/// AskTiming accumulator (no Instant probes fire).
fn disabled_ask_timing() -> super::ask::AskTiming {
    super::ask::AskTiming::disabled()
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;

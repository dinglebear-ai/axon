mod display;
mod scoring;
mod streaming;

use crate::core::config::Config;
use crate::core::http::http_client;
use crate::core::logging::log_warn;
use crate::services::acp_llm;
use std::error::Error;
use std::time::Instant;

use super::ask::{AskContext, build_ask_context, normalize_ask_answer};
use super::suggest::discover_crawl_suggestions;
use display::build_evaluate_json;
use scoring::{
    build_judge_reference, build_suggestion_focus, format_rag_sources, rag_underperformed,
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

/// Run the evaluate pipeline and return structured JSON without printing to stdout.
///
/// Forces `json_output = true` internally so the non-streaming path is used,
/// then builds the JSON payload via `build_evaluate_json()` and returns it.
pub async fn evaluate_payload(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut derived = cfg.clone();
    derived.json_output = true;
    let query = evaluate_query(&derived)?;
    let client = http_client()?;
    let eval_started = Instant::now();
    // Start warm sessions for all three LLM calls so their adapter cold-starts
    // overlap with retrieval work. warm1 → rag answer, warm2 → baseline, warm3 → judge.
    let make_warm = |label: &'static str| match acp_llm::warm_session(&derived, None) {
        Ok(w) => Some(w),
        Err(e) => {
            log_warn(&format!(
                "evaluate: {label} warm session failed to start: {e}"
            ));
            None
        }
    };
    let warm1 = make_warm("rag");
    let warm2 = make_warm("baseline");

    let ctx = build_evaluate_ask_context(&derived, &query).await?;
    let rag_future = run_rag_answer(&derived, client, &query, &ctx.context, warm1);
    let (rag_answer, rag_elapsed_ms, baseline_answer, baseline_elapsed_ms) = if derived
        .evaluate_retrieval_ab
    {
        // Retrieval A/B: replace the no-context baseline with a second RAG run that has
        // hybrid retrieval disabled, so the judge compares hybrid-RAG vs dense-only-RAG.
        let mut dense_cfg = derived.clone();
        dense_cfg.hybrid_search_enabled = false;
        let dense_ctx = build_evaluate_ask_context(&dense_cfg, &query).await?;
        let dense_future = run_rag_answer(&dense_cfg, client, &query, &dense_ctx.context, warm2);
        let (rag, dense) = tokio::try_join!(rag_future, dense_future)?;
        let (rag_answer, rag_elapsed_ms) = rag;
        let (dense_answer, dense_elapsed_ms) = dense;
        (rag_answer, rag_elapsed_ms, dense_answer, dense_elapsed_ms)
    } else {
        let baseline_future = run_baseline_answer(&derived, client, &query, warm2);
        let (rag, baseline) = tokio::try_join!(rag_future, baseline_future)?;
        let (rag_answer, rag_elapsed_ms) = rag;
        let (baseline_answer, baseline_elapsed_ms) = baseline;
        (
            rag_answer,
            rag_elapsed_ms,
            baseline_answer,
            baseline_elapsed_ms,
        )
    };
    let normalized_rag_answer = normalize_ask_answer(&derived, &query, &rag_answer, &ctx.context);
    let research_started = Instant::now();
    let warm3 = make_warm("judge");
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
    let (analysis_answer, analysis_elapsed_ms) =
        run_analysis(&derived, client, &judge_ctx, warm3).await;
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
    let crawl_enqueue_outcomes = Vec::new();
    let timing = EvalTiming {
        rag_elapsed_ms,
        baseline_elapsed_ms,
        research_elapsed_ms,
        analysis_elapsed_ms,
        total_elapsed_ms: eval_started.elapsed().as_millis(),
    };
    let eval_answers = EvalAnswers {
        rag: &normalized_rag_answer,
        baseline: &baseline_answer,
        analysis: &analysis_answer,
        crawl_suggestions: &crawl_suggestions,
        crawl_enqueue_outcomes: &crawl_enqueue_outcomes,
        ref_chunk_count,
        context_chars,
    };
    let source_urls = scoring::extract_source_urls(&ctx.diagnostic_sources);
    Ok(build_evaluate_json(
        &derived,
        &query,
        &ctx,
        &eval_answers,
        &timing,
        &source_urls,
    ))
}

fn evaluate_query(cfg: &Config) -> Result<String, Box<dyn Error>> {
    super::ask::validate_ask_llm_config(cfg)?;
    super::resolve_query_text(cfg).ok_or_else(|| "evaluate requires a question".into())
}

async fn build_evaluate_ask_context(
    cfg: &Config,
    query: &str,
) -> Result<AskContext, Box<dyn Error>> {
    let mut timing = disabled_ask_timing();
    Ok(build_ask_context(cfg, query, &mut timing).await?)
}

/// `evaluate` uses its own `EvaluateTiming` shape; ask sub-stage timings are
/// not surfaced from the evaluate path, so this helper produces a disabled
/// AskTiming accumulator (no Instant probes fire).
fn disabled_ask_timing() -> super::ask::AskTiming {
    super::ask::AskTiming::disabled()
}

#[cfg(test)]
mod tests {
    use super::display::{build_side_by_side_frame, wrap_fixed_width};
    use super::evaluate_query;
    use super::scoring::{build_suggestion_focus, rag_underperformed, score_totals_from_analysis};
    use crate::core::config::Config;

    #[test]
    fn wrap_fixed_width_respects_limit() {
        let lines = wrap_fixed_width("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn side_by_side_frame_contains_both_headers() {
        let frame = build_side_by_side_frame(100, "left answer", "right answer");
        assert!(frame[0].contains("WITH CONTEXT"));
        assert!(frame[0].contains("WITHOUT CONTEXT"));
        assert!(frame.iter().any(|line| line.contains("left answer")));
        assert!(frame.iter().any(|line| line.contains("right answer")));
    }

    #[test]
    fn score_totals_detects_rag_loss() {
        let analysis = "\
## Accuracy        RAG: 2/5 | Baseline: 4/5
## Relevance       RAG: 3/5 | Baseline: 4/5
## Completeness    RAG: 2/5 | Baseline: 4/5
## Specificity     RAG: 3/5 | Baseline: 4/5";
        let totals = score_totals_from_analysis(analysis).expect("expected parsed totals");
        assert!(totals.0 < totals.1);
        assert!(rag_underperformed(analysis));
    }

    #[test]
    fn score_totals_detects_rag_win() {
        let analysis = "\
## Accuracy        RAG: 5/5 | Baseline: 3/5
## Relevance       RAG: 5/5 | Baseline: 4/5";
        assert!(!rag_underperformed(analysis));
    }

    #[test]
    fn suggestion_focus_includes_weak_dimensions() {
        let analysis = "## Accuracy RAG: 2/5 | Baseline: 4/5";
        let focus = build_suggestion_focus("How does crawl fallback work?", analysis);
        assert!(focus.contains("How does crawl fallback work?"));
        assert!(focus.contains("RAG scored below baseline"));
        assert!(focus.contains("## Accuracy"));
    }

    #[test]
    fn evaluate_query_accepts_acp_only_config() {
        let mut cfg = Config::test_default();
        cfg.openai_base_url.clear();
        cfg.openai_model.clear();
        cfg.acp_adapter_cmd = Some("codex".to_string());
        cfg.query = Some("How does ACP validation work?".to_string());

        let query = evaluate_query(&cfg).expect("ACP-only config should pass");

        assert_eq!(query, "How does ACP validation work?");
    }

    #[test]
    fn evaluate_query_rejects_missing_acp_adapter_command() {
        let mut cfg = Config::test_default();
        cfg.openai_base_url.clear();
        cfg.openai_model.clear();
        cfg.acp_adapter_cmd = None;
        cfg.query = Some("How does ACP validation work?".to_string());

        let err = evaluate_query(&cfg).expect_err("missing adapter should fail");

        assert!(
            err.to_string().contains("AXON_ASK_AGENT"),
            "error should mention AXON_ASK_AGENT: {err}"
        );
    }
}

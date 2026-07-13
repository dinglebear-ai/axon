//! RAG/baseline/judge LLM answer acquisition for `evaluate`, ported from
//! legacy `axon_vector::ops::commands::evaluate::streaming`.
//!
//! The legacy side-by-side/inline parallel-streaming terminal UI
//! (`build_parallel_futures`, `run_parallel_answers_streaming`,
//! `handle_token_inline`, `handle_token_side_by_side`) was
//! `#[expect(dead_code)]` scaffolding never wired up to any caller, so it is
//! not reproduced here — only the three functions `evaluate_from_context`
//! actually calls are ported.

use axon_core::config::{Config, EvaluateResponsesMode};
use axon_core::logging::log_warn;
use std::time::Instant;

use crate::query::synthesis::completion::{
    JudgeContext, ask_llm_non_streaming, ask_llm_streaming, baseline_llm_non_streaming,
    baseline_llm_streaming, judge_llm_non_streaming, judge_llm_streaming,
};

pub(super) async fn run_rag_answer(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
    context: &str,
) -> Result<(String, u128), String> {
    let started = Instant::now();
    let stream_error = match ask_llm_streaming(cfg, client, query, context, !cfg.json_output).await
    {
        Ok(answer) => return Ok((answer, started.elapsed().as_millis())),
        Err(err) => err.to_string(),
    };
    log_warn(&format!(
        "rag streaming failed, falling back to non-streaming: {stream_error}"
    ));
    let answer = ask_llm_non_streaming(cfg, client, query, context)
        .await
        .map_err(|err| err.to_string())?;
    if !cfg.json_output {
        print!("{answer}");
    }
    Ok((answer, started.elapsed().as_millis()))
}

pub(super) async fn run_baseline_answer(
    cfg: &Config,
    client: &reqwest::Client,
    query: &str,
) -> Result<(String, u128), String> {
    let started = Instant::now();
    let stream_error = match baseline_llm_streaming(cfg, client, query, !cfg.json_output).await {
        Ok(answer) => return Ok((answer, started.elapsed().as_millis())),
        Err(err) => err.to_string(),
    };
    log_warn(&format!(
        "baseline streaming failed, falling back to non-streaming: {stream_error}"
    ));
    let answer = match baseline_llm_non_streaming(cfg, client, query).await {
        Ok(fallback) => {
            if !cfg.json_output {
                print!("{fallback}");
            }
            fallback
        }
        Err(e2) => {
            return Err(format!(
                "evaluate baseline failed after streaming and non-streaming attempts: streaming={stream_error}; fallback={e2}",
            ));
        }
    };
    Ok((answer, started.elapsed().as_millis()))
}

pub(super) async fn run_analysis(
    cfg: &Config,
    client: &reqwest::Client,
    judge_ctx: &JudgeContext<'_>,
) -> Result<(String, u128), String> {
    let started = Instant::now();
    let print_tokens =
        !cfg.json_output && cfg.evaluate_responses_mode != EvaluateResponsesMode::Events;
    let stream_error = match judge_llm_streaming(cfg, client, judge_ctx, print_tokens).await {
        Ok(answer) => return Ok((answer, started.elapsed().as_millis())),
        Err(err) => err.to_string(),
    };
    log_warn(&format!(
        "judge streaming failed, falling back to non-streaming: {stream_error}"
    ));
    let answer = match judge_llm_non_streaming(cfg, client, judge_ctx).await {
        Ok(fallback) => {
            if !cfg.json_output {
                print!("{fallback}");
            }
            fallback
        }
        Err(e2) => {
            return Err(format!(
                "evaluate judge failed after streaming and non-streaming attempts: streaming={stream_error}; fallback={e2}",
            ));
        }
    };
    Ok((answer, started.elapsed().as_millis()))
}

// No dedicated test sidecar: `run_rag_answer`/`run_baseline_answer`/
// `run_analysis` dispatch through `synthesis::completion`'s config-driven LLM
// calls (not runner-injectable), and the legacy crate had no direct unit
// tests for these three functions either — only for the abandoned tagged
// parallel-streaming scaffolding, which this port does not reproduce (see the
// module doc comment). Coverage for the citation/normalize/scoring logic each
// of these feeds into lives in `synthesis::normalize_tests` and
// `evaluate::scoring_tests`.

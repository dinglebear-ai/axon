// Scaffolding for the streaming evaluate UI — not all functions are wired up yet.
#![expect(
    dead_code,
    reason = "scaffolding for streaming evaluate UI — not all functions are wired up yet"
)]

use crate::core::config::{Config, EvaluateResponsesMode};
use crate::core::ui::{muted, primary};
use std::error::Error;
use std::io::Write as _;

use super::scoring::extract_source_urls;
use super::{EvalAnswers, EvalTiming};

pub(super) fn emit_event(value: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    println!("{}", serde_json::to_string(value)?);
    std::io::stdout().flush()?;
    Ok(())
}

pub(super) fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(80, 240))
        .unwrap_or(140)
}

pub(super) fn char_len(value: &str) -> usize {
    value.chars().count()
}

pub(super) fn pad_to_width(value: &str, width: usize) -> String {
    let current = char_len(value);
    if current >= width {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + (width - current));
    out.push_str(value);
    out.push_str(&" ".repeat(width - current));
    out
}

pub(super) fn wrap_fixed_width(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut acc = String::new();
        let mut count = 0usize;
        for ch in raw_line.chars() {
            if count == width {
                lines.push(acc);
                acc = String::new();
                count = 0;
            }
            acc.push(ch);
            count += 1;
        }
        lines.push(acc);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub(super) fn build_side_by_side_frame(
    total_width: usize,
    with_context: &str,
    without_context: &str,
) -> Vec<String> {
    let gutter = " │ ";
    let content_width = total_width.saturating_sub(2);
    let col_width = ((content_width.saturating_sub(gutter.len())) / 2).max(20);
    let left_header = pad_to_width("WITH CONTEXT", col_width);
    let right_header = pad_to_width("WITHOUT CONTEXT", col_width);
    let divider = format!(
        "{}{}{}",
        "─".repeat(col_width),
        "─┼─",
        "─".repeat(col_width)
    );
    let mut lines = vec![
        format!("  {left_header}{gutter}{right_header}"),
        format!("  {divider}"),
    ];
    let left_lines = wrap_fixed_width(with_context, col_width);
    let right_lines = wrap_fixed_width(without_context, col_width);
    let rows = left_lines.len().max(right_lines.len());
    for idx in 0..rows {
        let left = left_lines.get(idx).cloned().unwrap_or_default();
        let right = right_lines.get(idx).cloned().unwrap_or_default();
        lines.push(format!(
            "  {}{}{}",
            pad_to_width(&left, col_width),
            gutter,
            pad_to_width(&right, col_width)
        ));
    }
    lines
}

pub(super) fn repaint_frame(
    lines: &[String],
    previous_lines: usize,
) -> Result<usize, Box<dyn Error>> {
    if previous_lines > 0 {
        print!("\x1b[{}A\x1b[J", previous_lines);
    }
    for line in lines {
        println!("{line}");
    }
    std::io::stdout().flush()?;
    Ok(lines.len())
}

pub(super) fn emit_context_header(
    cfg: &Config,
    query: &str,
    ctx: &super::super::ask::AskContext,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        return Ok(());
    }
    if cfg.evaluate_responses_mode == EvaluateResponsesMode::Events {
        let source_urls = extract_source_urls(&ctx.diagnostic_sources);
        return emit_event(&serde_json::json!({
            "type": "evaluate_context_ready",
            "query": query,
            "responses_mode": cfg.evaluate_responses_mode.to_string(),
            "context": {
                "source_count": ctx.chunks_selected + ctx.full_docs_selected + ctx.supplemental_count,
                "source_urls": source_urls,
                "chars": ctx.context.len(),
                "retrieval_ms": ctx.retrieval_elapsed_ms,
                "context_build_ms": ctx.context_elapsed_ms,
            }
        }));
    }
    println!("{}", primary("Evaluate"));
    println!("  {} {}", primary("Question:"), query);
    println!(
        "  {} {} sources · {} chars  {}",
        primary("Context:"),
        ctx.chunks_selected + ctx.full_docs_selected + ctx.supplemental_count,
        ctx.context.len(),
        muted(&format!(
            "(retrieval={}ms · context={}ms)",
            ctx.retrieval_elapsed_ms, ctx.context_elapsed_ms
        ))
    );
    if cfg.ask_diagnostics {
        eprintln!(
            "  {} candidates={} reranked={} chunks={} full_docs={} supplemental={} context_chars={}",
            muted("Context detail:"),
            ctx.candidate_count,
            ctx.reranked_count,
            ctx.chunks_selected,
            ctx.full_docs_selected,
            ctx.supplemental_count,
            ctx.context.len()
        );
        for source in &ctx.diagnostic_sources {
            eprintln!("  • {source}");
        }
    }
    println!();
    match cfg.evaluate_responses_mode {
        EvaluateResponsesMode::Inline => println!(
            "{}",
            primary("── Parallel Answers (with and without context) ────────────────")
        ),
        EvaluateResponsesMode::SideBySide => println!(
            "{}",
            primary("── Parallel Answers (side-by-side) ───────────────────────────")
        ),
        EvaluateResponsesMode::Events => {}
    }
    Ok(())
}

pub(super) fn emit_analysis_header(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        return Ok(());
    }
    if cfg.evaluate_responses_mode == EvaluateResponsesMode::Events {
        return emit_event(&serde_json::json!({
            "type": "analysis_start",
        }));
    }
    println!();
    println!();
    println!(
        "{}",
        primary("── Analysis ───────────────────────────────────────────────────")
    );
    print!("  ");
    std::io::stdout().flush()?;
    Ok(())
}

pub(super) fn build_evaluate_json(
    cfg: &Config,
    query: &str,
    ctx: &super::super::ask::AskContext,
    answers: &EvalAnswers<'_>,
    timing: &EvalTiming,
    source_urls: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "query": query,
        "rag_answer": answers.rag,
        "baseline_answer": answers.baseline,
        "analysis_answer": answers.analysis,
        "source_urls": source_urls,
        "crawl_suggestions": answers.crawl_suggestions.iter().map(|s| serde_json::json!({
            "url": s.url,
            "reason": s.reason,
        })).collect::<Vec<_>>(),
        "crawl_enqueue_outcomes": answers.crawl_enqueue_outcomes.iter().map(|o| serde_json::json!({
            "url": o.url,
            "job_id": o.job_id,
            "error": o.error,
        })).collect::<Vec<_>>(),
        "ref_chunk_count": answers.ref_chunk_count,
        "diagnostics": if cfg.ask_diagnostics {
            serde_json::json!({
                "candidate_pool": ctx.candidate_count,
                "reranked_pool": ctx.reranked_count,
                "chunks_selected": ctx.chunks_selected,
                "full_docs_selected": ctx.full_docs_selected,
                "supplemental_selected": ctx.supplemental_count,
                "context_chars": answers.context_chars,
                "min_relevance_score": cfg.ask_min_relevance_score,
                "doc_fetch_concurrency": cfg.ask_doc_fetch_concurrency,
            })
        } else {
            serde_json::Value::Null
        },
        "timing_ms": {
            "retrieval": ctx.retrieval_elapsed_ms,
            "context_build": ctx.context_elapsed_ms,
            "rag_llm": timing.rag_elapsed_ms,
            "baseline_llm": timing.baseline_elapsed_ms,
            "research_elapsed_ms": timing.research_elapsed_ms,
            "analysis_llm_ms": timing.analysis_elapsed_ms,
            "total": timing.total_elapsed_ms,
        }
    })
}

fn emit_json_output(
    cfg: &Config,
    query: &str,
    ctx: &super::super::ask::AskContext,
    answers: &EvalAnswers<'_>,
    timing: &EvalTiming,
    source_urls: &[String],
) -> Result<(), Box<dyn Error>> {
    let json = build_evaluate_json(cfg, query, ctx, answers, timing, source_urls);
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn emit_events_output(
    query: &str,
    ctx: &super::super::ask::AskContext,
    answers: &EvalAnswers<'_>,
    timing: &EvalTiming,
    source_urls: &[String],
) -> Result<(), Box<dyn Error>> {
    emit_event(&serde_json::json!({
        "type": "evaluate_complete",
        "query": query,
        "rag_answer": answers.rag,
        "baseline_answer": answers.baseline,
        "analysis_answer": answers.analysis,
        "source_urls": source_urls,
        "crawl_suggestions": answers.crawl_suggestions.iter().map(|s| serde_json::json!({
            "url": s.url,
            "reason": s.reason,
        })).collect::<Vec<_>>(),
        "crawl_enqueue_outcomes": answers.crawl_enqueue_outcomes.iter().map(|o| serde_json::json!({
            "url": o.url,
            "job_id": o.job_id,
            "error": o.error,
        })).collect::<Vec<_>>(),
        "timing_ms": {
            "retrieval": ctx.retrieval_elapsed_ms,
            "context_build": ctx.context_elapsed_ms,
            "rag_llm": timing.rag_elapsed_ms,
            "baseline_llm": timing.baseline_elapsed_ms,
            "research_elapsed_ms": timing.research_elapsed_ms,
            "analysis_llm_ms": timing.analysis_elapsed_ms,
            "total": timing.total_elapsed_ms,
        }
    }))
}

fn emit_terminal_output(
    answers: &EvalAnswers<'_>,
    timing: &EvalTiming,
) -> Result<(), Box<dyn Error>> {
    if !answers.crawl_suggestions.is_empty() {
        println!();
        println!();
        println!(
            "{}",
            primary("── Suggested Sources To Crawl (RAG scored below baseline) ───")
        );
        for (idx, suggestion) in answers.crawl_suggestions.iter().enumerate() {
            println!("  {}. {}", idx + 1, suggestion.url);
            println!("     {}", muted(&suggestion.reason));
        }
        if !answers.crawl_enqueue_outcomes.is_empty() {
            println!();
            println!("  {}", muted("Auto-crawl enqueue results:"));
            for outcome in answers.crawl_enqueue_outcomes {
                match (&outcome.job_id, &outcome.error) {
                    (Some(job_id), _) => println!("    • {} -> {}", outcome.url, muted(job_id)),
                    (_, Some(err)) => println!("    • {} -> {}", outcome.url, muted(err)),
                    _ => {}
                }
            }
        }
    }
    println!();
    println!();
    println!(
        "  {} rag_llm={}ms | baseline_llm={}ms | research={}ms | analysis_llm={}ms | total={}ms",
        muted("Timing:"),
        timing.rag_elapsed_ms,
        timing.baseline_elapsed_ms,
        timing.research_elapsed_ms,
        timing.analysis_elapsed_ms,
        timing.total_elapsed_ms
    );
    Ok(())
}

pub(super) fn emit_evaluate_output(
    cfg: &Config,
    query: &str,
    ctx: &super::super::ask::AskContext,
    answers: &EvalAnswers<'_>,
    timing: &EvalTiming,
) -> Result<(), Box<dyn Error>> {
    let source_urls = extract_source_urls(&ctx.diagnostic_sources);
    if cfg.json_output {
        return emit_json_output(cfg, query, ctx, answers, timing, &source_urls);
    }
    if cfg.evaluate_responses_mode == EvaluateResponsesMode::Events {
        return emit_events_output(query, ctx, answers, timing, &source_urls);
    }
    emit_terminal_output(answers, timing)
}

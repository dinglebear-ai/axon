//! Judge-reference retrieval + analysis-text scoring helpers, ported from
//! legacy `axon_vector::ops::commands::evaluate::scoring`.
//!
//! `build_judge_reference` now retrieves its independent reference material
//! through [`retrieval_ask_context`] (the `axon-retrieval` engine) instead of
//! the legacy `build_ask_context` reranker — same substitution the `ask`/main
//! `evaluate` RAG context already made.
//!
//! `structured_scores_from_analysis`/`AxisScore`/`StructuredScores` are NOT
//! ported: they only fed the legacy JSON-only `evaluate_payload` wrapper's
//! extra `scores` field, which the typed `EvaluateResult` (no `scores` field)
//! never carried — that enrichment was already dropped when `evaluate`
//! switched to the typed `evaluate_result_with_context` entry point.

use std::error::Error;

use axon_core::config::Config;

use crate::context::ServiceContext;
use crate::query::ask_retrieval::retrieval_ask_context;

const NO_REFERENCE: &str = "No reference material available.";

pub(super) async fn build_judge_reference(
    ctx: &ServiceContext,
    cfg: &Config,
    question: &str,
) -> Result<(String, usize), Box<dyn Error>> {
    let ask_ctx = retrieval_ask_context(ctx, cfg, question, "evaluate_judge_reference").await?;
    let ref_count =
        ask_ctx.chunks_selected + ask_ctx.full_docs_selected + ask_ctx.supplemental_count;
    if ref_count == 0 || ask_ctx.context.trim().is_empty() {
        return Ok((NO_REFERENCE.to_string(), 0));
    }
    Ok((ask_ctx.context, ref_count))
}

pub(super) fn parse_first_score(value: &str, label: &str) -> Option<f64> {
    let start = value.find(label)?;
    let tail = &value[start + label.len()..];
    let mut number = String::new();
    let mut seen = false;
    for ch in tail.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
            seen = true;
            continue;
        }
        if seen {
            break;
        }
    }
    number.parse::<f64>().ok()
}

pub(super) fn score_totals_from_analysis(analysis: &str) -> Option<(f64, f64)> {
    let mut rag_total = 0.0f64;
    let mut baseline_total = 0.0f64;
    let mut score_rows = 0usize;
    for line in analysis.lines() {
        let rag = parse_first_score(line, "RAG: ");
        let base = parse_first_score(line, "Baseline: ");
        if let (Some(r), Some(b)) = (rag, base) {
            rag_total += r;
            baseline_total += b;
            score_rows += 1;
        }
    }
    if score_rows == 0 {
        return None;
    }
    Some((rag_total, baseline_total))
}

pub(super) fn rag_underperformed(analysis: &str) -> bool {
    score_totals_from_analysis(analysis)
        .map(|(rag, baseline)| rag + 0.001 < baseline)
        .unwrap_or(false)
}

pub(super) fn build_suggestion_focus(query: &str, analysis: &str) -> String {
    let mut weak_dimensions = Vec::new();
    for line in analysis.lines() {
        let rag = parse_first_score(line, "RAG: ");
        let base = parse_first_score(line, "Baseline: ");
        if let (Some(r), Some(b)) = (rag, base)
            && r + 0.001 < b
        {
            weak_dimensions.push(line.trim().to_string());
        }
    }
    if weak_dimensions.is_empty() {
        return query.to_string();
    }
    format!(
        "{query}\n\nRAG scored below baseline in these areas:\n- {}",
        weak_dimensions.join("\n- ")
    )
}

pub(super) fn format_rag_sources(diagnostic_sources: &[String]) -> String {
    if diagnostic_sources.is_empty() {
        return "None available".to_string();
    }
    diagnostic_sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let url = s.split_once(" url=").map_or(s.as_str(), |(_, u)| u);
            format!("[S{}] {}", i + 1, url)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn extract_source_urls(diagnostic_sources: &[String]) -> Vec<String> {
    diagnostic_sources
        .iter()
        .map(|s| {
            s.split_once(" url=")
                .map_or_else(|| s.to_string(), |(_, u)| u.to_string())
        })
        .collect()
}

#[cfg(test)]
#[path = "scoring_tests.rs"]
mod tests;

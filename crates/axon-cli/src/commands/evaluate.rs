use crate::commands::common::truncate_chars;
use crate::commands::resolve_input_text;
use axon_core::config::{Config, EvaluateResponsesMode};
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::query as query_service;
use std::error::Error;
use std::fmt;

/// CLI shim for the evaluate command.
pub async fn run_evaluate(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let question = resolve_input_text(cfg).ok_or("evaluate requires a question")?;
    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=evaluate query_len={}", question.len()));
    }

    let result = query_service::evaluate(cfg, &question)
        .await
        .map_err(|err| -> Box<dyn Error> { Box::new(EvaluateCliError(err)) })?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    let payload = serde_json::to_value(&result)?;
    print_evaluate_output(cfg, &payload, &question)?;
    Ok(())
}

pub(crate) fn print_evaluate_output(
    cfg: &Config,
    payload: &serde_json::Value,
    question: &str,
) -> Result<(), Box<dyn Error>> {
    match cfg.evaluate_responses_mode {
        EvaluateResponsesMode::Events => {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "type": "evaluate_complete",
                    "payload": payload,
                }))?
            );
        }
        EvaluateResponsesMode::SideBySide => {
            print_side_by_side_answers(payload);
            print_human_evaluate_output(payload, question);
        }
        EvaluateResponsesMode::Inline => print_human_evaluate_output(payload, question),
    }
    Ok(())
}

fn print_side_by_side_answers(payload: &serde_json::Value) {
    let rag = payload["rag_answer"].as_str().unwrap_or("");
    let baseline = payload["baseline_answer"].as_str().unwrap_or("");
    if rag.is_empty() && baseline.is_empty() {
        return;
    }
    println!(
        "  {:<42} {}",
        primary("WITH CONTEXT"),
        primary("WITHOUT CONTEXT")
    );
    println!(
        "{}",
        muted(
            "  ──────────────────────────────────────────┼──────────────────────────────────────────"
        )
    );
    for line in build_side_by_side_rows(rag, baseline, 42) {
        println!("{line}");
    }
    println!();
}

fn build_side_by_side_rows(left: &str, right: &str, width: usize) -> Vec<String> {
    let left_lines = wrap_fixed_width(left, width);
    let right_lines = wrap_fixed_width(right, width);
    let rows = left_lines.len().max(right_lines.len());
    (0..rows)
        .map(|idx| {
            format!(
                "  {:width$} │ {:width$}",
                left_lines.get(idx).map(String::as_str).unwrap_or(""),
                right_lines.get(idx).map(String::as_str).unwrap_or(""),
                width = width
            )
        })
        .collect()
}

fn wrap_fixed_width(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let chars = raw.chars().collect::<Vec<_>>();
        for chunk in chars.chunks(width.max(1)) {
            lines.push(chunk.iter().collect());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn print_human_evaluate_output(payload: &serde_json::Value, question: &str) {
    let truncated_q = truncate_chars(question, 80);
    let ellipsis = if question.len() > 80 { "..." } else { "" };
    println!("  {} {}{ellipsis}", primary("Question:"), truncated_q);
    println!();

    // Parse scores from the analysis_answer text (format: "## Accuracy RAG: 4/5 | Baseline: 3/5")
    let analysis = payload["analysis_answer"].as_str().unwrap_or("");
    let dimensions = ["Accuracy", "Relevance", "Completeness", "Specificity"];
    let mut found_scores = false;
    for dim in &dimensions {
        if let Some((rag, baseline)) = parse_dimension_scores(analysis, dim) {
            println!(
                "  {} {} {} {} {}",
                muted(&format!("{:<16}", format!("{dim}:"))),
                muted("RAG:"),
                accent(&rag),
                muted("| Baseline:"),
                accent(&baseline),
            );
            found_scores = true;
        }
    }

    // Derive verdict from score totals
    if found_scores {
        let verdict = derive_verdict(analysis);
        println!(
            "  {} {}",
            muted(&format!("{:<16}", "Verdict:")),
            accent(verdict)
        );
    } else {
        // Fallback: show raw analysis if scores could not be parsed
        println!("  {} {}", primary("Analysis:"), analysis);
    }

    // Timing summary
    if let Some(timing) = payload.get("timing_ms") {
        println!();
        println!(
            "  {} rag={}ms | baseline={}ms | analysis={}ms | total={}ms",
            muted("Timing:"),
            timing["rag_llm"].as_u64().unwrap_or(0),
            timing["baseline_llm"].as_u64().unwrap_or(0),
            timing["analysis_llm_ms"].as_u64().unwrap_or(0),
            timing["total"].as_u64().unwrap_or(0),
        );
    }

    // Source count
    if let Some(urls) = payload["source_urls"].as_array()
        && !urls.is_empty()
    {
        println!("  {} {}", muted("Sources:"), urls.len());
    }
}

/// Parse "RAG: N/M" and "Baseline: N/M" from a line containing the given dimension.
fn parse_dimension_scores(analysis: &str, dimension: &str) -> Option<(String, String)> {
    for line in analysis.lines() {
        if !line.contains(dimension) {
            continue;
        }
        let rag = extract_score(line, "RAG: ")?;
        let baseline = extract_score(line, "Baseline: ")?;
        return Some((rag, baseline));
    }
    None
}

/// Extract a score string like "4/5" after the given label.
fn extract_score(line: &str, label: &str) -> Option<String> {
    let start = line.find(label)?;
    let tail = &line[start + label.len()..];
    let score: String = tail
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '/' || *c == '.')
        .collect();
    if score.is_empty() { None } else { Some(score) }
}

/// Derive a human-readable verdict from the analysis score totals.
fn derive_verdict(analysis: &str) -> &'static str {
    // Reuse the same parsing logic as the vector layer
    let mut rag_total = 0.0f64;
    let mut baseline_total = 0.0f64;
    let mut rows = 0usize;
    for line in analysis.lines() {
        let rag = parse_first_number(line, "RAG: ");
        let base = parse_first_number(line, "Baseline: ");
        if let (Some(r), Some(b)) = (rag, base) {
            rag_total += r;
            baseline_total += b;
            rows += 1;
        }
    }
    if rows == 0 {
        return "UNKNOWN";
    }
    let diff = rag_total - baseline_total;
    if diff > 2.0 {
        "STRONG — RAG significantly outperforms baseline"
    } else if diff > 0.0 {
        "GOOD — RAG outperforms baseline"
    } else if (diff).abs() < f64::EPSILON {
        "NEUTRAL — RAG and baseline are tied"
    } else if diff > -2.0 {
        "WEAK — baseline outperforms RAG"
    } else {
        "POOR — baseline significantly outperforms RAG"
    }
}

fn parse_first_number(line: &str, label: &str) -> Option<f64> {
    let start = line.find(label)?;
    let tail = &line[start + label.len()..];
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

#[derive(Debug)]
struct EvaluateCliError(Box<dyn Error + Send + Sync>);

impl fmt::Display for EvaluateCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for EvaluateCliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;

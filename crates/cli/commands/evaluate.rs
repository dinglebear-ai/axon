use crate::crates::cli::commands::common::truncate_chars;
use crate::crates::cli::commands::resolve_input_text;
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_info;
use crate::crates::core::ui::{muted, primary};
use crate::crates::services::query as query_service;
use std::error::Error;

/// CLI shim for the evaluate command.
pub async fn run_evaluate(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let question = resolve_input_text(cfg).ok_or("evaluate requires a question")?;
    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=evaluate query_len={}", question.len()));
    }

    let result = query_service::evaluate(cfg, &question).await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
        return Ok(());
    }

    print_human_evaluate_output(&result.payload, &question);
    Ok(())
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
                "  {:<16} RAG: {} | Baseline: {}",
                format!("{dim}:"),
                rag,
                baseline
            );
            found_scores = true;
        }
    }

    // Derive verdict from score totals
    if found_scores {
        let verdict = derive_verdict(analysis);
        println!("  {:<16} {}", "Verdict:", verdict);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dimension_scores_extracts_rag_and_baseline() {
        let analysis = "## Accuracy        RAG: 4/5 | Baseline: 3/5\n## Relevance       RAG: 5/5 | Baseline: 4/5";
        let (rag, baseline) = parse_dimension_scores(analysis, "Accuracy").unwrap();
        assert_eq!(rag, "4/5");
        assert_eq!(baseline, "3/5");
    }

    #[test]
    fn parse_dimension_scores_returns_none_for_missing() {
        let analysis = "## Accuracy RAG: 4/5 | Baseline: 3/5";
        assert!(parse_dimension_scores(analysis, "Specificity").is_none());
    }

    #[test]
    fn derive_verdict_strong_rag() {
        let analysis =
            "## Accuracy RAG: 5/5 | Baseline: 2/5\n## Relevance RAG: 5/5 | Baseline: 2/5";
        assert!(derive_verdict(analysis).contains("STRONG"));
    }

    #[test]
    fn derive_verdict_weak_rag() {
        let analysis =
            "## Accuracy RAG: 2/5 | Baseline: 4/5\n## Relevance RAG: 2/5 | Baseline: 4/5";
        assert!(derive_verdict(analysis).contains("POOR"));
    }

    #[test]
    fn derive_verdict_unknown_no_scores() {
        assert_eq!(derive_verdict("no scores here"), "UNKNOWN");
    }
}

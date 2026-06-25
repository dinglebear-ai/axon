use axon_core::config::Config;
use std::error::Error;

const NO_REFERENCE: &str = "No reference material available.";
const SCORE_AXES: &[(&str, &str)] = &[
    ("accuracy", "accuracy"),
    ("relevance", "relevance"),
    ("completeness", "completeness"),
    ("specificity", "specificity"),
];

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(super) struct AxisScore {
    pub axis: &'static str,
    pub rag: f64,
    pub baseline: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(super) struct StructuredScores {
    pub status: &'static str,
    pub axes: Vec<AxisScore>,
    pub rag_total: Option<f64>,
    pub baseline_total: Option<f64>,
    pub winner: Option<&'static str>,
}

pub(super) async fn build_judge_reference(
    cfg: &Config,
    question: &str,
) -> Result<(String, usize), Box<dyn Error>> {
    let mut ask_timing = super::super::ask::AskTiming::disabled();
    let ctx = super::super::ask::build_ask_context(cfg, question, &mut ask_timing).await?;
    let ref_count = ctx.chunks_selected + ctx.full_docs_selected + ctx.supplemental_count;
    if ref_count == 0 || ctx.context.trim().is_empty() {
        return Ok((NO_REFERENCE.to_string(), 0));
    }
    Ok((ctx.context, ref_count))
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

pub(super) fn structured_scores_from_analysis(analysis: &str) -> StructuredScores {
    let axes = SCORE_AXES
        .iter()
        .filter_map(|(axis, needle)| {
            analysis.lines().find_map(|line| {
                let lower = line.to_ascii_lowercase();
                if !lower.contains(needle) {
                    return None;
                }
                let rag = parse_first_score(line, "RAG: ")?;
                let baseline = parse_first_score(line, "Baseline: ")?;
                Some(AxisScore {
                    axis,
                    rag,
                    baseline,
                })
            })
        })
        .collect::<Vec<_>>();

    if axes.is_empty() {
        return StructuredScores {
            status: "parse_failed",
            axes,
            rag_total: None,
            baseline_total: None,
            winner: None,
        };
    }

    let rag_total = axes.iter().map(|score| score.rag).sum::<f64>();
    let baseline_total = axes.iter().map(|score| score.baseline).sum::<f64>();
    let winner = if (rag_total - baseline_total).abs() < f64::EPSILON {
        Some("tie")
    } else if rag_total > baseline_total {
        Some("rag")
    } else {
        Some("baseline")
    };
    StructuredScores {
        status: if axes.len() == SCORE_AXES.len() {
            "parsed"
        } else {
            "partial"
        },
        axes,
        rag_total: Some(rag_total),
        baseline_total: Some(baseline_total),
        winner,
    }
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

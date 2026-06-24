use super::display::{build_side_by_side_frame, wrap_fixed_width};
use super::evaluate_query;
use super::scoring::{
    build_suggestion_focus, rag_underperformed, score_totals_from_analysis,
    structured_scores_from_analysis,
};
use axon_core::config::Config;

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
fn structured_scores_parse_expected_axes() {
    let analysis = "\
## Accuracy        RAG: 5/5 | Baseline: 3/5
## Relevance       RAG: 4/5 | Baseline: 4/5
## Completeness    RAG: 5/5 | Baseline: 2/5
## Specificity     RAG: 4/5 | Baseline: 3/5
## Verdict
RAG is better.";
    let scores = structured_scores_from_analysis(analysis);
    assert_eq!(scores.status, "parsed");
    assert_eq!(scores.axes.len(), 4);
    assert_eq!(scores.rag_total, Some(18.0));
    assert_eq!(scores.baseline_total, Some(12.0));
    assert_eq!(scores.winner, Some("rag"));
}

#[test]
fn structured_scores_reports_parse_failure_explicitly() {
    let scores = structured_scores_from_analysis("judge returned prose only");
    assert_eq!(scores.status, "parse_failed");
    assert!(scores.axes.is_empty());
    assert_eq!(scores.winner, None);
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
fn evaluate_query_accepts_gemini_config() {
    let mut cfg = Config::test_default();
    cfg.query = Some("How does Gemini validation work?".to_string());

    let query = evaluate_query(&cfg).expect("Gemini config should pass");

    assert_eq!(query, "How does Gemini validation work?");
}

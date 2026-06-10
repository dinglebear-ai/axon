use super::{
    build_suggestion_focus, extract_source_urls, format_rag_sources, parse_first_score,
    rag_underperformed, score_totals_from_analysis, structured_scores_from_analysis,
};

// ── parse_first_score — malformed / edge-case inputs ─────────────────────────

#[test]
fn parse_first_score_returns_none_on_empty_string() {
    assert!(parse_first_score("", "RAG: ").is_none());
}

#[test]
fn parse_first_score_returns_none_when_label_absent() {
    let text = "Accuracy score 7.5 out of 10";
    assert!(parse_first_score(text, "RAG: ").is_none());
}

#[test]
fn parse_first_score_returns_none_when_no_digits_follow_label() {
    let text = "RAG: no digits here at all";
    assert!(parse_first_score(text, "RAG: ").is_none());
}

#[test]
fn parse_first_score_returns_none_on_non_numeric_suffix() {
    // "RAG: xyz" — 'x' is not a digit so scan ends immediately with an empty number
    let text = "RAG: xyz";
    assert!(parse_first_score(text, "RAG: ").is_none());
}

#[test]
fn parse_first_score_parses_integer_value() {
    let text = "RAG: 8 Baseline: 7";
    assert_eq!(parse_first_score(text, "RAG: "), Some(8.0));
}

#[test]
fn parse_first_score_parses_decimal_value() {
    let text = "RAG: 7.5 Baseline: 6.0";
    assert_eq!(parse_first_score(text, "RAG: "), Some(7.5));
}

#[test]
fn parse_first_score_picks_first_occurrence_only() {
    // Two "RAG: " occurrences — must return the first
    let text = "RAG: 9 something RAG: 5";
    assert_eq!(parse_first_score(text, "RAG: "), Some(9.0));
}

// ── score_totals_from_analysis — malformed / partial inputs ──────────────────

#[test]
fn score_totals_returns_none_on_empty_analysis() {
    assert!(score_totals_from_analysis("").is_none());
}

#[test]
fn score_totals_returns_none_when_no_paired_scores() {
    // Line has RAG but no Baseline
    let analysis = "This answer had RAG: 8 but no comparison.";
    assert!(score_totals_from_analysis(analysis).is_none());
}

#[test]
fn score_totals_returns_none_on_garbage_judge_output() {
    let analysis = "The LLM returned markdown with ```json``` blocks and no scores at all.";
    assert!(score_totals_from_analysis(analysis).is_none());
}

#[test]
fn score_totals_sums_multiple_score_rows() {
    let analysis = "\
        Accuracy: RAG: 8 Baseline: 7\n\
        Relevance: RAG: 9 Baseline: 6\n\
        Completeness: RAG: 7 Baseline: 8\n";
    let (rag, base) = score_totals_from_analysis(analysis).expect("should parse three rows");
    assert!((rag - 24.0).abs() < 1e-9);
    assert!((base - 21.0).abs() < 1e-9);
}

#[test]
fn score_totals_ignores_lines_missing_either_score() {
    let analysis = "\
        Accuracy: RAG: 8 Baseline: 7\n\
        Missing baseline line RAG: 9\n\
        Completeness: RAG: 7 Baseline: 6\n";
    let (rag, base) = score_totals_from_analysis(analysis).expect("two complete rows");
    assert!((rag - 15.0).abs() < 1e-9);
    assert!((base - 13.0).abs() < 1e-9);
}

// ── structured_scores_from_analysis ──────────────────────────────────────────

#[test]
fn structured_scores_parse_failed_on_empty_input() {
    let scores = structured_scores_from_analysis("");
    assert_eq!(scores.status, "parse_failed");
    assert!(scores.axes.is_empty());
    assert!(scores.rag_total.is_none());
    assert!(scores.baseline_total.is_none());
    assert!(scores.winner.is_none());
}

#[test]
fn structured_scores_parse_failed_on_garbage_output() {
    let garbage = "I'm sorry but I cannot evaluate this answer as requested. Please try again.";
    let scores = structured_scores_from_analysis(garbage);
    assert_eq!(scores.status, "parse_failed");
    assert!(scores.axes.is_empty());
}

#[test]
fn structured_scores_partial_when_only_some_axes_present() {
    // Only accuracy and relevance — completeness and specificity missing
    let analysis = "\
        Accuracy: RAG: 8 Baseline: 7\n\
        Relevance: RAG: 9 Baseline: 6\n";
    let scores = structured_scores_from_analysis(analysis);
    assert_eq!(scores.status, "partial");
    assert_eq!(scores.axes.len(), 2);
    assert!(scores.rag_total.is_some());
}

#[test]
fn structured_scores_parsed_when_all_four_axes_present() {
    let analysis = "\
        Accuracy: RAG: 8 Baseline: 7\n\
        Relevance: RAG: 9 Baseline: 6\n\
        Completeness: RAG: 7 Baseline: 8\n\
        Specificity: RAG: 6 Baseline: 5\n";
    let scores = structured_scores_from_analysis(analysis);
    assert_eq!(scores.status, "parsed");
    assert_eq!(scores.axes.len(), 4);
    assert_eq!(scores.winner, Some("rag"));
}

#[test]
fn structured_scores_winner_baseline_when_baseline_higher() {
    let analysis = "\
        Accuracy: RAG: 5 Baseline: 8\n\
        Relevance: RAG: 6 Baseline: 9\n\
        Completeness: RAG: 5 Baseline: 7\n\
        Specificity: RAG: 4 Baseline: 6\n";
    let scores = structured_scores_from_analysis(analysis);
    assert_eq!(scores.winner, Some("baseline"));
}

#[test]
fn structured_scores_winner_tie_when_totals_equal() {
    let analysis = "\
        Accuracy: RAG: 7 Baseline: 7\n\
        Relevance: RAG: 8 Baseline: 8\n\
        Completeness: RAG: 6 Baseline: 6\n\
        Specificity: RAG: 5 Baseline: 5\n";
    let scores = structured_scores_from_analysis(analysis);
    assert_eq!(scores.winner, Some("tie"));
}

// ── rag_underperformed ────────────────────────────────────────────────────────

#[test]
fn rag_underperformed_false_on_empty_analysis() {
    assert!(!rag_underperformed(""));
}

#[test]
fn rag_underperformed_false_when_rag_wins() {
    let analysis = "\
        Accuracy: RAG: 9 Baseline: 7\n\
        Relevance: RAG: 8 Baseline: 6\n";
    assert!(!rag_underperformed(analysis));
}

#[test]
fn rag_underperformed_true_when_baseline_wins() {
    let analysis = "\
        Accuracy: RAG: 5 Baseline: 8\n\
        Relevance: RAG: 6 Baseline: 9\n";
    assert!(rag_underperformed(analysis));
}

// ── build_suggestion_focus ────────────────────────────────────────────────────

#[test]
fn build_suggestion_focus_returns_query_unchanged_when_no_weak_dimensions() {
    let analysis = "Accuracy: RAG: 9 Baseline: 7\nRelevance: RAG: 8 Baseline: 6\n";
    let focus = build_suggestion_focus("my question", analysis);
    assert_eq!(focus, "my question");
}

#[test]
fn build_suggestion_focus_appends_weak_dimensions() {
    let analysis = "Accuracy: RAG: 5 Baseline: 8\nRelevance: RAG: 9 Baseline: 6\n";
    let focus = build_suggestion_focus("my question", analysis);
    assert!(focus.starts_with("my question"));
    assert!(focus.contains("RAG scored below baseline"));
    assert!(focus.contains("RAG: 5 Baseline: 8"));
}

// ── format_rag_sources / extract_source_urls ─────────────────────────────────

#[test]
fn format_rag_sources_returns_none_available_for_empty_slice() {
    assert_eq!(format_rag_sources(&[]), "None available");
}

#[test]
fn format_rag_sources_extracts_url_from_tagged_entry() {
    let sources = vec!["chunk url=https://docs.example.com/page".to_string()];
    let out = format_rag_sources(&sources);
    assert!(out.contains("[S1] https://docs.example.com/page"));
}

#[test]
fn format_rag_sources_uses_full_string_when_no_url_tag() {
    let sources = vec!["bare string without url tag".to_string()];
    let out = format_rag_sources(&sources);
    assert!(out.contains("[S1] bare string without url tag"));
}

#[test]
fn extract_source_urls_returns_url_portion_only() {
    let sources = vec![
        "score=0.9 url=https://docs.example.com/a".to_string(),
        "score=0.8 url=https://docs.example.com/b".to_string(),
    ];
    let urls = extract_source_urls(&sources);
    assert_eq!(
        urls,
        vec!["https://docs.example.com/a", "https://docs.example.com/b",]
    );
}

#[test]
fn extract_source_urls_falls_back_to_full_string_when_no_tag() {
    let sources = vec!["plain entry no url tag".to_string()];
    let urls = extract_source_urls(&sources);
    assert_eq!(urls, vec!["plain entry no url tag"]);
}

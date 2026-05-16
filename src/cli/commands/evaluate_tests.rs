use super::*;

#[test]
fn parse_dimension_scores_extracts_rag_and_baseline() {
    let analysis =
        "## Accuracy        RAG: 4/5 | Baseline: 3/5\n## Relevance       RAG: 5/5 | Baseline: 4/5";
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
    let analysis = "## Accuracy RAG: 5/5 | Baseline: 2/5\n## Relevance RAG: 5/5 | Baseline: 2/5";
    assert!(derive_verdict(analysis).contains("STRONG"));
}

#[test]
fn derive_verdict_weak_rag() {
    let analysis = "## Accuracy RAG: 2/5 | Baseline: 4/5\n## Relevance RAG: 2/5 | Baseline: 4/5";
    assert!(derive_verdict(analysis).contains("POOR"));
}

#[test]
fn derive_verdict_unknown_no_scores() {
    assert_eq!(derive_verdict("no scores here"), "UNKNOWN");
}

#[test]
fn side_by_side_rows_include_both_answers() {
    let rows = build_side_by_side_rows("abcdef", "uvwxyz", 3);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("abc"));
    assert!(rows[0].contains("uvw"));
    assert!(rows[1].contains("def"));
    assert!(rows[1].contains("xyz"));
}

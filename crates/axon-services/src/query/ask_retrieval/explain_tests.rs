use super::*;
use axon_core::ask_explain::{
    AskExplainFilterDecisionKind, AskExplainInsertionMode, AskExplainMode,
    AskExplainSelectionDecisionKind,
};
use axon_core::config::Config;

fn hit(uri: &str, id: &str, score: f64, text: &str) -> QueryServiceHit {
    QueryServiceHit {
        canonical_uri: uri.to_string(),
        chunk_id: id.to_string(),
        score,
        text: text.to_string(),
    }
}

#[test]
fn selected_candidates_are_kept_and_ranked() {
    let cfg = Config::test_default();
    let hits = vec![
        hit("https://example.com/a", "a#0", 0.9, "alpha body"),
        hit("https://example.org/b", "b#0", 0.7, "beta body"),
        hit("https://example.net/c", "c#0", 0.5, "gamma body"),
    ];
    let trace = build_explain_trace(&cfg, "question", &hits, 2, "Sources:\n...");

    assert_eq!(trace.mode, AskExplainMode::ExplainOnly);
    assert!(trace.llm_skipped);
    assert_eq!(trace.candidates.len(), 3);

    let selected: Vec<_> = trace.candidates.iter().take(2).collect();
    for (idx, candidate) in selected.iter().enumerate() {
        assert!(
            candidate
                .filter_decisions
                .iter()
                .any(|d| d.kind == AskExplainFilterDecisionKind::Kept)
        );
        assert_eq!(
            candidate.selection_decisions[0].kind,
            AskExplainSelectionDecisionKind::SelectedTopChunk
        );
        assert_eq!(candidate.selected_context_rank, Some(idx + 1));
        assert_eq!(
            candidate.insertion_mode,
            Some(AskExplainInsertionMode::TopChunk)
        );
        assert_eq!(candidate.retrieval_score, candidate.rerank_score);
    }

    let dropped = &trace.candidates[2];
    assert!(
        dropped
            .filter_decisions
            .iter()
            .all(|d| d.kind != AskExplainFilterDecisionKind::Kept)
    );
    assert_eq!(
        dropped.selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::NotSelected
    );
    assert_eq!(dropped.selected_context_rank, None);
    assert_eq!(
        dropped.insertion_mode,
        Some(AskExplainInsertionMode::NotSelected)
    );
}

#[test]
fn candidate_trace_truncates_at_limit() {
    let cfg = Config::test_default();
    let hits: Vec<_> = (0..(CANDIDATE_TRACE_LIMIT + 5))
        .map(|i| {
            hit(
                &format!("https://example.com/{i}"),
                &format!("c{i}"),
                0.5,
                "body",
            )
        })
        .collect();
    let trace = build_explain_trace(&cfg, "q", &hits, 3, "Sources:\n...");

    assert_eq!(trace.candidates.len(), CANDIDATE_TRACE_LIMIT);
    assert_eq!(trace.candidate_trace_limit, CANDIDATE_TRACE_LIMIT);
    assert!(trace.candidate_trace_truncated);
}

#[test]
fn context_final_source_order_matches_selected_prefix() {
    let cfg = Config::test_default();
    let hits = vec![
        hit("https://example.com/a", "a#0", 0.9, "alpha"),
        hit("https://example.org/b", "b#0", 0.7, "beta"),
    ];
    let trace = build_explain_trace(&cfg, "q", &hits, 1, "Sources:\nabc");

    assert_eq!(trace.context.final_source_order.len(), 1);
    assert_eq!(
        trace.context.final_source_order[0].url,
        "https://example.com/a"
    );
    assert_eq!(trace.context.final_source_order[0].source_id, "S1");
    assert!(trace.context.truncated_by_budget);
    assert_eq!(
        trace.context.context_chars_used,
        "Sources:\nabc".chars().count()
    );
}

#[test]
fn no_candidates_yields_empty_trace_without_panicking() {
    let cfg = Config::test_default();
    let trace = build_explain_trace(&cfg, "q", &[], 0, "Sources:\n");
    assert!(trace.candidates.is_empty());
    assert!(trace.context.final_source_order.is_empty());
    assert!(!trace.context.truncated_by_budget);
}

#[test]
fn snippet_truncates_long_text() {
    let long = "x".repeat(SNIPPET_MAX_CHARS + 50);
    let short = snippet(&long);
    assert!(short.ends_with('…'));
    assert_eq!(short.chars().count(), SNIPPET_MAX_CHARS + 1);
}

#[test]
fn snippet_preserves_short_text_unchanged() {
    assert_eq!(snippet("  hello world  "), "hello world");
}

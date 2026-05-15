use super::*;
use crate::services::types::AskExplainSelectionDecisionKind;

fn candidate(url: &str, score: f64) -> ranking::AskCandidate {
    ranking::AskCandidate {
        score,
        url: url.to_string(),
        path: url.to_string(),
        chunk_text: "trace test chunk text".to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score: score,
    }
}

fn decision_kinds(selection: &ContextCandidateSelection) -> Vec<AskExplainSelectionDecisionKind> {
    selection
        .decisions
        .iter()
        .map(|decision| decision.kind)
        .collect()
}

#[test]
fn context_trace_marks_full_doc_skip_without_suppressing_top_chunk() {
    let reranked = vec![candidate("https://code.claude.com/docs/en/plugins", 0.90)];
    let planned = HashSet::new();
    let selected_top = selected_top_chunk_indices(&reranked, &[0], &planned, 1);
    let decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: &reranked,
        top_chunk_indices: &[0],
        selected_top_chunk_indices: &selected_top,
        planned_full_doc_urls: &planned,
        top_full_doc_indices: &[0],
        inserted_full_doc_urls: &HashSet::new(),
        supplemental_indices: &[],
        supplemental_count: 0,
        full_doc_fetch_skipped: true,
    });

    let kinds = decision_kinds(&decisions[0]);
    assert!(kinds.contains(&AskExplainSelectionDecisionKind::SelectedTopChunk));
    assert!(kinds.contains(&AskExplainSelectionDecisionKind::PlannedFullDoc));
    assert!(kinds.contains(&AskExplainSelectionDecisionKind::SkippedFullDocFetchSkipped));
}

#[test]
fn context_trace_marks_planned_suppression_and_budget_skips() {
    let reranked = vec![
        candidate("https://a.test/docs", 0.90),
        candidate("https://b.test/docs", 0.80),
        candidate("https://c.test/docs", 0.70),
    ];
    let planned = HashSet::from(["https://a.test/docs".to_string()]);
    let selected_top = selected_top_chunk_indices(&reranked, &[0, 1], &planned, 0);
    let decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: &reranked,
        top_chunk_indices: &[0, 1],
        selected_top_chunk_indices: &selected_top,
        planned_full_doc_urls: &planned,
        top_full_doc_indices: &[0],
        inserted_full_doc_urls: &HashSet::new(),
        supplemental_indices: &[2],
        supplemental_count: 0,
        full_doc_fetch_skipped: false,
    });

    assert!(
        decision_kinds(&decisions[0])
            .contains(&AskExplainSelectionDecisionKind::SkippedPlannedFullDoc)
    );
    assert!(
        decision_kinds(&decisions[1]).contains(&AskExplainSelectionDecisionKind::SkippedBudget)
    );
    assert!(
        decision_kinds(&decisions[2]).contains(&AskExplainSelectionDecisionKind::SkippedBudget)
    );
}

#[test]
fn context_trace_final_source_order_matches_prompt_order() {
    let entries = vec![
        (
            0.8,
            "## Top Chunk [S9]: https://b.test/docs\n\nbody".to_string(),
        ),
        (
            0.7,
            "## Source Document [S2]: https://a.test/docs\n\nbody".to_string(),
        ),
    ];
    let order = final_source_order_from_entries(&entries);

    assert_eq!(order.len(), 2);
    assert_eq!(order[0].source_id, "S1");
    assert_eq!(order[0].url, "https://b.test/docs");
    assert_eq!(order[0].tier, "top_chunk");
    assert_eq!(order[0].sort_rank, 1);
    assert_eq!(order[0].sort_score, 0.8);
    assert_eq!(order[1].source_id, "S2");
    assert_eq!(order[1].url, "https://a.test/docs");
    assert_eq!(order[1].tier, "full_doc");
    assert_eq!(order[1].sort_rank, 2);
    assert_eq!(order[1].sort_score, 0.7);
}

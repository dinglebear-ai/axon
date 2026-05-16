use super::*;
use crate::services::types::{
    AskExplainContextSource, AskExplainInsertionMode, AskExplainSelectionDecisionKind,
};

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

fn source(source_id: &str, url: &str, tier: &str, sort_rank: usize) -> AskExplainContextSource {
    AskExplainContextSource {
        source_id: source_id.to_string(),
        url: url.to_string(),
        tier: tier.to_string(),
        sort_rank,
        sort_score: 0.0,
    }
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
        final_source_order: &[],
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
        final_source_order: &[],
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

#[test]
fn context_trace_emits_selection_metadata() {
    let reranked = vec![
        candidate("https://a.test/top", 0.90),
        candidate("https://b.test/full", 0.80),
        candidate("https://c.test/planned", 0.70),
        candidate("https://d.test/supplemental", 0.60),
        candidate("https://e.test/not-selected", 0.50),
    ];
    let planned = HashSet::from([
        "https://b.test/full".to_string(),
        "https://c.test/planned".to_string(),
    ]);
    let inserted = HashSet::from(["https://b.test/full".to_string()]);
    let final_source_order = vec![
        source("S1", "https://a.test/top", "top_chunk", 1),
        source("S2", "https://b.test/full", "full_doc", 2),
        source("S3", "https://d.test/supplemental", "supplemental", 3),
    ];

    let decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: &reranked,
        top_chunk_indices: &[0],
        selected_top_chunk_indices: &[0],
        planned_full_doc_urls: &planned,
        top_full_doc_indices: &[1, 2],
        inserted_full_doc_urls: &inserted,
        supplemental_indices: &[3],
        supplemental_count: 1,
        full_doc_fetch_skipped: false,
        final_source_order: &final_source_order,
    });

    assert_eq!(
        decisions[0].metadata.insertion_mode,
        Some(AskExplainInsertionMode::TopChunk)
    );
    assert_eq!(decisions[0].metadata.selected_context_rank, Some(1));
    assert_eq!(
        decisions[1].metadata.insertion_mode,
        Some(AskExplainInsertionMode::InsertedFullDoc)
    );
    assert_eq!(decisions[1].metadata.planned_full_doc_rank, Some(1));
    assert_eq!(decisions[1].metadata.selected_context_rank, Some(2));
    assert_eq!(
        decisions[2].metadata.insertion_mode,
        Some(AskExplainInsertionMode::PlannedFullDoc)
    );
    assert_eq!(decisions[2].metadata.planned_full_doc_rank, Some(2));
    assert_eq!(decisions[2].metadata.selected_context_rank, None);
    assert_eq!(
        decisions[3].metadata.insertion_mode,
        Some(AskExplainInsertionMode::Supplemental)
    );
    assert_eq!(decisions[3].metadata.selected_context_rank, Some(3));
    assert_eq!(
        decisions[4].metadata.insertion_mode,
        Some(AskExplainInsertionMode::NotSelected)
    );
}

#[test]
fn context_trace_prioritizes_supplemental_mode_over_planned_only() {
    let reranked = vec![candidate("https://a.test/docs", 0.90)];
    let planned = HashSet::from(["https://a.test/docs".to_string()]);
    let final_source_order = vec![source("S1", "https://a.test/docs", "supplemental", 1)];

    let decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: &reranked,
        top_chunk_indices: &[],
        selected_top_chunk_indices: &[],
        planned_full_doc_urls: &planned,
        top_full_doc_indices: &[0],
        inserted_full_doc_urls: &HashSet::new(),
        supplemental_indices: &[0],
        supplemental_count: 1,
        full_doc_fetch_skipped: false,
        final_source_order: &final_source_order,
    });

    assert_eq!(
        decisions[0].metadata.insertion_mode,
        Some(AskExplainInsertionMode::Supplemental)
    );
    assert_eq!(decisions[0].metadata.planned_full_doc_rank, Some(1));
    assert_eq!(decisions[0].metadata.selected_context_rank, Some(1));
}

#[test]
fn context_trace_ranks_duplicate_url_by_context_tier() {
    let reranked = vec![
        candidate("https://a.test/docs", 0.90),
        candidate("https://a.test/docs", 0.80),
    ];
    let planned = HashSet::from(["https://a.test/docs".to_string()]);
    let inserted = HashSet::from(["https://a.test/docs".to_string()]);
    let final_source_order = vec![
        source("S1", "https://a.test/docs", "top_chunk", 1),
        source("S2", "https://a.test/docs", "full_doc", 2),
    ];

    let decisions = build_context_selection_decisions(ContextSelectionInputs {
        reranked: &reranked,
        top_chunk_indices: &[0],
        selected_top_chunk_indices: &[0],
        planned_full_doc_urls: &planned,
        top_full_doc_indices: &[1],
        inserted_full_doc_urls: &inserted,
        supplemental_indices: &[],
        supplemental_count: 0,
        full_doc_fetch_skipped: false,
        final_source_order: &final_source_order,
    });

    assert_eq!(decisions[0].metadata.selected_context_rank, Some(1));
    assert_eq!(decisions[1].metadata.selected_context_rank, Some(2));
}

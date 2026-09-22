use super::*;
use axon_core::ask_explain::{
    AskExplainFilterDecisionKind, AskExplainInsertionMode, AskExplainMode,
    AskExplainSelectionDecisionKind,
};
use axon_core::config::Config;
use axon_core::llm::LlmBackendKind;
use axon_retrieval::QueryServiceHit;

fn hit(uri: &str, id: &str, score: f64, text: &str) -> QueryServiceHit {
    QueryServiceHit {
        canonical_uri: uri.to_string(),
        chunk_id: id.to_string(),
        score,
        text: text.to_string(),
        citation: super::super::tests::citation(uri),
    }
}

fn cfg() -> Config {
    Config {
        llm_backend: LlmBackendKind::OpenAiCompat,
        openai_model: "chatgpt-browser-medium".to_string(),
        ..Config::test_default()
    }
}

#[test]
fn explain_reports_real_rerank_and_selection_order() {
    let mut cfg = cfg();
    cfg.ask_chunk_limit = 2;
    cfg.ask_authoritative_domains = vec!["example.org".to_string()];
    cfg.ask_authoritative_boost = 0.5;
    let mut ranked = super::super::ranking::rank_candidates(
        &cfg,
        "Claude hooks",
        vec![
            hit(
                "https://example.com/docs/hooks",
                "a#0",
                0.9,
                "Claude hooks body",
            ),
            hit(
                "https://example.org/docs/hooks",
                "b#0",
                0.7,
                "Claude hooks body",
            ),
            hit(
                "https://example.net/docs/hooks",
                "c#0",
                0.5,
                "Claude hooks body",
            ),
        ],
        true,
    );
    let ctx = super::super::build_ask_context_from_ranking(&cfg, &mut ranked, 5);
    let trace = build_explain_trace(&cfg, "Claude hooks", &ranked, &ctx.context);

    assert_eq!(trace.mode, AskExplainMode::ExplainOnly);
    assert!(trace.llm_skipped);
    assert_eq!(trace.candidates.len(), 3);
    assert_eq!(trace.citations.len(), 2);
    assert_eq!(trace.candidates[0].url, "https://example.org/docs/hooks");
    assert!(trace.candidates[0].rerank_score > trace.candidates[0].retrieval_score);
    assert!(
        trace.candidates[0]
            .score_components
            .iter()
            .any(|component| component.name == "authority_boost" && component.value > 0.0)
    );

    for (index, candidate) in trace.candidates.iter().take(2).enumerate() {
        assert!(
            candidate
                .filter_decisions
                .iter()
                .any(|decision| decision.kind == AskExplainFilterDecisionKind::Kept)
        );
        assert_eq!(
            candidate.selection_decisions[0].kind,
            AskExplainSelectionDecisionKind::SelectedTopChunk
        );
        assert_eq!(candidate.selected_context_rank, Some(index + 1));
        assert_eq!(
            candidate.insertion_mode,
            Some(AskExplainInsertionMode::TopChunk)
        );
    }

    let unselected = &trace.candidates[2];
    assert!(
        unselected
            .filter_decisions
            .iter()
            .any(|decision| decision.kind == AskExplainFilterDecisionKind::Kept)
    );
    assert_eq!(
        unselected.selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::NotSelected
    );
    assert_eq!(
        unselected.insertion_mode,
        Some(AskExplainInsertionMode::NotSelected)
    );
}

#[test]
fn explain_surfaces_low_signal_session_filter() {
    let cfg = cfg();
    let ranked = super::super::ranking::rank_candidates(
        &cfg,
        "Claude hooks",
        vec![hit(
            "session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa",
            "session#0",
            0.99,
            "Claude hooks from prior session",
        )],
        true,
    );
    let trace = build_explain_trace(
        &cfg,
        "Claude hooks",
        &ranked,
        "Sources:
",
    );

    assert!(
        trace.candidates[0]
            .filter_decisions
            .iter()
            .any(|decision| decision.kind == AskExplainFilterDecisionKind::DroppedLowSignal)
    );
    assert_eq!(
        trace.candidates[0].selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::NotSelected
    );
}

#[test]
fn candidate_trace_truncates_at_limit() {
    let cfg = cfg();
    let hits = (0..(CANDIDATE_TRACE_LIMIT + 5))
        .map(|index| {
            hit(
                &format!("https://example.com/docs/hooks/{index}"),
                &format!("c{index}"),
                0.5,
                "Claude hooks documentation",
            )
        })
        .collect();
    let ranked = super::super::ranking::rank_candidates(&cfg, "Claude hooks", hits, true);
    let trace = build_explain_trace(
        &cfg,
        "Claude hooks",
        &ranked,
        "Sources:
",
    );

    assert_eq!(trace.candidates.len(), CANDIDATE_TRACE_LIMIT);
    assert_eq!(trace.candidate_trace_limit, CANDIDATE_TRACE_LIMIT);
    assert!(trace.candidate_trace_truncated);
}

#[test]
fn context_final_source_order_matches_actual_selection() {
    let mut cfg = cfg();
    cfg.ask_chunk_limit = 1;
    let mut ranked = super::super::ranking::rank_candidates(
        &cfg,
        "Claude hooks",
        vec![
            hit(
                "https://example.com/docs/hooks",
                "a#0",
                0.9,
                "Claude hooks alpha",
            ),
            hit(
                "https://example.org/docs/hooks",
                "b#0",
                0.7,
                "Claude hooks beta",
            ),
        ],
        true,
    );
    let ctx = super::super::build_ask_context_from_ranking(&cfg, &mut ranked, 5);
    let trace = build_explain_trace(&cfg, "Claude hooks", &ranked, &ctx.context);

    assert_eq!(trace.context.final_source_order.len(), 1);
    assert_eq!(
        trace.context.final_source_order[0].url,
        "https://example.com/docs/hooks"
    );
    assert_eq!(trace.context.final_source_order[0].source_id, "S1");
    assert_eq!(
        trace.context.context_chars_used,
        ctx.context.chars().count()
    );
}

#[test]
fn dense_explain_reports_cosine_full_doc_mode() {
    let mut cfg = cfg();
    cfg.hybrid_search_enabled = false;
    let mut ranked = super::super::ranking::rank_candidates(
        &cfg,
        "Claude hooks",
        vec![hit(
            "https://example.com/docs/hooks",
            "dense#0",
            0.9,
            "Claude hooks documentation",
        )],
        false,
    );
    let ctx = super::super::build_ask_context_from_ranking(&cfg, &mut ranked, 5);
    let trace = build_explain_trace(&cfg, "Claude hooks", &ranked, &ctx.context);

    assert_eq!(
        trace.retrieval.score_kind,
        axon_core::ask_explain::AskExplainScoreKind::NamedDense
    );
    assert_eq!(
        trace.context.full_doc_fetch_mode,
        axon_core::ask_explain::AskExplainFullDocFetchMode::Cosine
    );
    assert_eq!(
        trace.context.full_doc_fetch_skip_reason,
        axon_core::ask_explain::AskExplainFullDocFetchSkipReason::NotSupportedByRetrievalEngine
    );
}

#[test]
fn explain_marks_byte_budget_unenforced_for_character_bounded_context() {
    let mut cfg = cfg();
    cfg.ask_max_context_chars = 10_000;
    let mut ranked = super::super::ranking::rank_candidates(
        &cfg,
        "Claude hooks",
        vec![hit(
            "https://example.com/docs/hooks",
            "unicode#0",
            0.9,
            "Claude hooks documentation 😀 café 日本語",
        )],
        true,
    );
    let ctx = super::super::build_ask_context_from_ranking(&cfg, &mut ranked, 5);
    let trace = build_explain_trace(&cfg, "Claude hooks", &ranked, &ctx.context);

    assert_eq!(
        trace.context.context_char_budget,
        ranked.effective_budget.max_context_chars
    );
    assert_eq!(trace.context.context_bytes_budget, 0);
    assert!(trace.context.context_bytes_used > trace.context.context_chars_used);
}

#[test]
fn explain_marks_selected_chunk_truncation_as_budget_truncation() {
    let mut cfg = cfg();
    cfg.ask_chunk_limit = 4;
    cfg.ask_max_context_chars = 12_000;
    let huge = "Claude hooks formatter PostToolUse ".repeat(2_000);
    let mut ranked = super::super::ranking::rank_candidates(
        &cfg,
        "How should Claude hooks run a formatter after edits?",
        vec![hit(
            "https://code.claude.com/docs/en/hooks-guide.md",
            "hooks#0",
            0.9,
            &huge,
        )],
        true,
    );
    let ctx = super::super::build_ask_context_from_ranking(&cfg, &mut ranked, 5);
    let trace = build_explain_trace(
        &cfg,
        "How should Claude hooks run a formatter after edits?",
        &ranked,
        &ctx.context,
    );

    assert!(trace.context.truncated_by_budget);
    assert!(
        ranked.candidates[0]
            .selection_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("context truncation"))
    );
}

#[test]
fn no_candidates_yields_empty_trace_without_panicking() {
    let cfg = cfg();
    let ranked = super::super::ranking::rank_candidates(&cfg, "Claude hooks", Vec::new(), true);
    let trace = build_explain_trace(
        &cfg,
        "Claude hooks",
        &ranked,
        "Sources:
",
    );
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

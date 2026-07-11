use super::super::timing::AskTimingSlot;
use super::*;

fn sample_ctx() -> AskContext {
    AskContext::from_retrieval(
        "Sources:\n## Top Chunk [S1]: https://example.com/docs\n\n<retrieved_content trust=\"evidence_only\">\nbody\n</retrieved_content>".to_string(),
        3,
        1,
        12,
        vec!["example.com".to_string()],
        &["https://example.com/docs".to_string()],
        Vec::new(),
    )
}

#[test]
fn assemble_ask_result_without_diagnostics_omits_diagnostics_field() {
    let cfg = Config::default();
    let ctx = sample_ctx();
    let timing = AskTiming::new(false, std::time::Instant::now());
    let answer = "The answer [S1].\n\n## Sources\n- [S1] https://example.com/docs";

    let result = assemble_ask_result(&cfg, "how?", &ctx, answer, 50, 100, &timing, false);

    assert_eq!(result.query, "how?");
    assert_eq!(result.answer, answer);
    assert!(result.diagnostics.is_none());
    assert_eq!(result.timing_ms.retrieval, ctx.retrieval_elapsed_ms);
    assert_eq!(result.timing_ms.llm, 50);
    assert_eq!(result.timing_ms.total, 100);
    assert!(result.explain.is_none());
    let validation = result
        .citation_validation
        .expect("citation validation present");
    assert!(validation.valid);
}

#[test]
fn assemble_ask_result_with_diagnostics_reports_context_stats() {
    let cfg = Config::default();
    let ctx = sample_ctx();
    let timing = AskTiming::new(true, std::time::Instant::now());
    let answer = "The answer [S1].\n\n## Sources\n- [S1] https://example.com/docs";

    let result = assemble_ask_result(&cfg, "how?", &ctx, answer, 50, 100, &timing, true);

    let diagnostics = result.diagnostics.expect("diagnostics present");
    assert_eq!(diagnostics.candidate_pool, ctx.candidate_count);
    assert_eq!(diagnostics.chunks_selected, ctx.chunks_selected);
    assert_eq!(diagnostics.full_doc_fetch_skip_reason, "retrieval_engine");
    assert_eq!(diagnostics.top_domains, ctx.top_domains);
}

#[test]
fn build_timing_disabled_only_reports_streamed_and_ttft() {
    let mut timing = AskTiming::new(false, std::time::Instant::now());
    timing.set_streamed(true);
    timing.set_ttft(7);

    let wire = build_timing(1, 2, 3, 4, &timing);

    assert_eq!(wire.retrieval, 1);
    assert_eq!(wire.context_build, 2);
    assert_eq!(wire.llm, 3);
    assert_eq!(wire.total, 4);
    assert_eq!(wire.streamed, Some(true));
    assert_eq!(wire.llm_ttft_ms, Some(7));
    assert!(wire.tei_embed_ms.is_none());
}

#[test]
fn build_timing_enabled_reports_normalize_and_llm_total() {
    let mut timing = AskTiming::new(true, std::time::Instant::now());
    timing.set(AskTimingSlot::LlmTotal, 42);
    timing.set(AskTimingSlot::Normalize, 5);

    let wire = build_timing(1, 2, 3, 4, &timing);

    assert_eq!(wire.llm_total_ms, Some(42));
    assert_eq!(wire.normalize_ms, Some(5));
}

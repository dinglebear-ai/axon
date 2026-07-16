use super::super::timing::AskTimingSlot;
use super::*;

fn sample_ctx() -> AskContext {
    let mut ctx = AskContext::from_retrieval(
        "Sources:\n## Top Chunk [S1]: https://example.com/docs\n\n<retrieved_content trust=\"evidence_only\">\nbody\n</retrieved_content>".to_string(),
        3,
        1,
        12,
        vec!["example.com".to_string()],
        &["https://example.com/docs".to_string()],
        Vec::new(),
    );
    ctx.citations.push(
        serde_json::from_value(serde_json::json!({
            "source_id": "source-test",
            "source_item_key": "docs",
            "generation": "1",
            "document_id": "document-test",
            "chunk_id": "chunk-test",
            "job_id": "00000000-0000-0000-0000-000000000001",
            "canonical_uri": "https://example.com/docs",
            "source_range": { "line_start": 1, "line_end": 1 },
            "redaction": {
                "redaction_status": "clean",
                "redaction_version": "test-v1",
                "visibility": "public",
                "redacted_field_count": 0,
                "dropped_field_count": 0,
                "detector_count": 0,
                "detector_names": []
            }
        }))
        .expect("canonical citation fixture"),
    );
    ctx
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
    assert_eq!(result.citations, ctx.citations);
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

fn sample_trace() -> AskExplainTrace {
    use axon_core::ask_explain::{
        AskExplainContext, AskExplainFullDocFetchMode, AskExplainFullDocFetchSkipReason,
        AskExplainMode, AskExplainRetrieval,
    };
    AskExplainTrace {
        mode: AskExplainMode::ExplainOnly,
        retrieval: AskExplainRetrieval {
            query: "how?".to_string(),
            keyword_query: "how?".to_string(),
            dual_search: false,
            collection: "axon".to_string(),
            candidate_limit: 1,
            hybrid_search_enabled: true,
            hybrid_candidate_limit: 150,
            score_kind: axon_core::ask_explain::AskExplainScoreKind::Rrf,
            vector_mode: "named_hybrid_rrf".to_string(),
            sparse_query_status: None,
        },
        candidates: Vec::new(),
        citations: Vec::new(),
        context: AskExplainContext {
            planned_full_doc_urls: Vec::new(),
            full_doc_fetch_errors: Vec::new(),
            full_doc_fetch_skipped: true,
            full_doc_fetch_skip_reason: AskExplainFullDocFetchSkipReason::Disabled,
            full_doc_fetch_mode: AskExplainFullDocFetchMode::Rrf,
            final_source_order: Vec::new(),
            context_char_budget: 1000,
            context_chars_used: 10,
            context_bytes_budget: 1000,
            context_bytes_used: 10,
            rendered_context: None,
            truncated_by_budget: false,
        },
        candidate_trace_limit: 50,
        candidate_trace_truncated: false,
        llm_skipped: true,
    }
}

#[test]
fn assemble_explain_result_skips_llm_and_carries_trace() {
    let cfg = Config::default();
    let ctx = sample_ctx();
    let trace = sample_trace();

    let result = assemble_explain_result(&cfg, "how?", &ctx, trace.clone(), 42);

    assert_eq!(result.query, "how?");
    assert_eq!(result.answer, "");
    assert!(result.citation_validation.is_none());
    assert_eq!(result.explain, Some(trace));
    assert_eq!(result.citations, ctx.citations);
    assert_eq!(result.timing_ms.llm, 0);
    assert_eq!(result.timing_ms.total, 42);
    assert_eq!(result.timing_ms.retrieval, ctx.retrieval_elapsed_ms);
    assert!(result.timing_ms.streamed.is_none());
    // Explain mode always populates diagnostics, regardless of `cfg.ask_diagnostics`.
    assert!(result.diagnostics.is_some());
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

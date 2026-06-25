use super::normalize::{
    extract_cited_source_ids, normalize_ask_answer, parse_context_source_map,
    summarize_citation_validation,
};
use super::validate_ask_llm_config;
use axon_api::AskResult;
use axon_core::config::Config;
use axon_core::llm::LlmBackendKind;

fn cfg() -> Config {
    Config::default()
}

#[test]
fn extract_cited_source_ids_deduplicates_ids() {
    let ids = extract_cited_source_ids("A [S1] B [S2][S1] C [s3] D [S11, S13]");
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 11, 13]);
}

#[test]
fn normalize_ask_answer_replaces_sources_with_deduped_section() {
    let context = "Sources:\n## Top Chunk [S1]: https://docs.a.dev/guide\n\n---\n\n## Top Chunk [S2]: https://docs.b.dev/api";
    let raw = "Use command X [S2] and Y [S1].\n\n## Sources\n- [S1] dup\n- [S1] dup";
    let normalized = normalize_ask_answer(&cfg(), "how do I use this?", raw, context);
    assert!(normalized.contains("Use command X [S2] and Y [S1]."));
    assert!(normalized.contains("## Sources"));
    assert!(normalized.contains("- [S1] https://docs.a.dev/guide"));
    assert!(normalized.contains("- [S2] https://docs.b.dev/api"));
    assert!(!normalized.contains("dup"));
}

#[test]
fn normalize_ask_answer_dedupes_sources_by_url() {
    let context = "Sources:\n## Top Chunk [S1]: https://same.dev/docs\n\n---\n\n## Top Chunk [S9]: https://same.dev/docs";
    let raw = "Use this flow [S1][S9].";
    let normalized = normalize_ask_answer(&cfg(), "how do I use this?", raw, context);
    assert!(normalized.contains("Use this flow [S1][S1]."));
    assert!(normalized.contains("- [S1] https://same.dev/docs"));
    assert!(!normalized.contains("- [S9] https://same.dev/docs"));
}

#[test]
fn normalize_ask_answer_dedupes_canonical_url_variants() {
    let context = "Sources:\n## Source Document [S1]: https://code.claude.com/docs/en/plugins.md\n\n---\n\n## Top Chunk [S2]: https://code.claude.com/docs/en/plugins\n\n---\n\n## Source Document [S3]: https://code.claude.com/docs/en/plugin-marketplaces.md";
    let raw = "Create the manifest and skill file [S1][S2], then publish or share via marketplaces when needed [S3].";
    let normalized = normalize_ask_answer(
        &cfg(),
        "how do I create a Claude Code plugin?",
        raw,
        context,
    );
    assert!(
        normalized.contains("Create the manifest and skill file [S1][S1], then publish or share via marketplaces when needed [S2].")
    );
    assert!(normalized.contains("- [S1] https://code.claude.com/docs/en/plugins.md"));
    assert!(normalized.contains("- [S2] https://code.claude.com/docs/en/plugin-marketplaces.md"));
    assert!(!normalized.contains("- [S3]"));
}

#[test]
fn normalize_ask_answer_renumbers_sparse_source_ids_for_display() {
    let context = "Sources:\n## Top Chunk [S11]: https://docs.example.com/hooks";
    let raw = "Hooks run at configured lifecycle events [S11].";
    let normalized = normalize_ask_answer(&cfg(), "how do hooks work?", raw, context);
    assert!(normalized.contains("Hooks run at configured lifecycle events [S1]."));
    assert!(normalized.contains("## Sources\n- [S1] https://docs.example.com/hooks"));
    assert!(!normalized.contains("[S11]"));
}

#[test]
fn normalize_ask_answer_renumbers_grouped_source_ids_for_display() {
    let context = "Sources:\n## Top Chunk [S11]: https://docs.example.com/hooks\n\n---\n\n## Top Chunk [S13]: https://docs.example.com/settings";
    let raw = "Hooks and settings interact at lifecycle boundaries [S11, S13].";
    let normalized = normalize_ask_answer(&cfg(), "how do hooks work?", raw, context);
    assert!(normalized.contains("Hooks and settings interact at lifecycle boundaries [S1, S2]."));
    assert!(normalized.contains("- [S1] https://docs.example.com/hooks"));
    assert!(normalized.contains("- [S2] https://docs.example.com/settings"));
    assert!(!normalized.contains("[S11, S13]"));
}

#[test]
fn normalize_ask_answer_formats_insufficient_evidence_when_uncited() {
    let context = "Sources:\n## Top Chunk [S1]: https://docs.example.com/guide";
    let raw = "I think this probably works, but not sure.";
    let normalized = normalize_ask_answer(&cfg(), "what is this?", raw, context);
    assert!(normalized.starts_with(raw));
    assert!(normalized.contains("## Citation Validation Failed"));
    assert!(normalized.contains("Answer contained no source citations."));
    assert!(normalized.contains("## Retrieved Sources\n- [S1] https://docs.example.com/guide"));
}

#[test]
fn normalize_ask_answer_keeps_insufficient_evidence_when_uncited_without_context() {
    let raw = "I think this probably works, but not sure.";
    let normalized = normalize_ask_answer(&cfg(), "what is this?", raw, "");
    assert!(normalized.starts_with("Insufficient evidence in indexed sources"));
    assert!(normalized.contains("## Sources\n- None cited from retrieved context."));
}

#[test]
fn normalize_ask_answer_formats_insufficient_evidence_when_flagged_in_body() {
    let context = "Sources:\n## Top Chunk [S2]: https://docs.example.com/guide";
    let raw = "The indexed sources are insufficient to answer this question [S2].";
    let normalized = normalize_ask_answer(&cfg(), "what is this?", raw, context);
    assert!(normalized.starts_with("Insufficient evidence in indexed sources"));
    assert!(normalized.contains("## Why"));
    assert!(normalized.contains("## Sources\n- [S2] https://docs.example.com/guide"));
}

#[test]
fn parse_context_source_map_reads_source_headers() {
    let context = "Sources:\n## Top Chunk [S1]: https://a.dev\n\n---\n\n## Source Document [S2]: https://b.dev";
    let map = parse_context_source_map(context);
    assert_eq!(map.get(&1).map(|s| s.as_str()), Some("https://a.dev"));
    assert_eq!(map.get(&2).map(|s| s.as_str()), Some("https://b.dev"));
}

#[test]
fn non_trivial_answer_requires_minimum_citation_count() {
    let mut cfg = cfg();
    cfg.ask_min_citations_nontrivial = 2;
    let context = "Sources:\n## Top Chunk [S1]: https://docs.example.com/guide";
    let raw = "Step one do this. Step two do that. Step three validate and deploy. This guidance is comprehensive and should be followed in production. Add staged rollouts, canary checks, and automated rollback criteria for every release [S1].";
    let normalized = normalize_ask_answer(
        &cfg,
        "how do I deploy this service safely in production environments?",
        raw,
        context,
    );
    assert!(normalized.starts_with(raw));
    assert!(normalized.contains("## Citation Validation Failed"));
    assert!(normalized.contains("requires at least 2 unique citations"));
}

#[test]
fn summarize_citation_validation_reports_valid_sources() {
    let answer = "Do the thing [S1][S2].\n\n## Sources\n- [S1] https://docs.example.com/a\n- [S2] https://docs.example.com/b";

    let summary = summarize_citation_validation(answer);

    assert!(summary.valid);
    assert!(summary.issues.is_empty());
    assert_eq!(summary.canonical_citation_count, 2);
}

#[test]
fn summarize_citation_validation_reports_failure_reasons() {
    let answer = "Do the thing.\n\n## Citation Validation Failed\n- Answer contained no source citations.\n- Non-trivial answer requires at least 2 unique citations; found 0.\n\n## Retrieved Sources\n- [S1] https://docs.example.com/a";

    let summary = summarize_citation_validation(answer);

    assert!(!summary.valid);
    assert_eq!(
        summary.issues,
        vec![
            "Answer contained no source citations.".to_string(),
            "Non-trivial answer requires at least 2 unique citations; found 0.".to_string(),
        ]
    );
    assert_eq!(summary.canonical_citation_count, 1);
}

#[test]
fn validate_ask_llm_config_accepts_default_gemini_config() {
    let cfg = Config::test_default();

    let result = validate_ask_llm_config(&cfg);

    assert!(result.is_ok(), "Gemini config should pass validation");
}

#[test]
fn validate_ask_llm_config_accepts_openai_compat_config() {
    let mut cfg = Config::test_default();
    cfg.llm_backend = LlmBackendKind::OpenAiCompat;
    cfg.openai_base_url = "http://llama-cpp:8080/v1".to_string();
    cfg.openai_model = "gemma".to_string();

    let result = validate_ask_llm_config(&cfg);

    assert!(
        result.is_ok(),
        "OpenAI-compatible config should pass validation"
    );
}

#[test]
fn validate_ask_llm_config_rejects_openai_compat_without_base_url() {
    let mut cfg = Config::test_default();
    cfg.llm_backend = LlmBackendKind::OpenAiCompat;
    cfg.openai_model = "gemma".to_string();

    let err = validate_ask_llm_config(&cfg).expect_err("base URL should be required");

    assert!(err.to_string().contains("AXON_OPENAI_BASE_URL"));
}

#[test]
fn validate_ask_llm_config_accepts_codex_app_server_config() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    validate_ask_llm_config(&cfg).expect("codex config should validate");
}

#[test]
fn validate_ask_llm_config_rejects_empty_codex_cmd() {
    let cfg = Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "   ".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    };

    let err = validate_ask_llm_config(&cfg).unwrap_err();
    assert!(err.to_string().contains("AXON_CODEX_CMD"));
}

#[test]
fn ask_result_defaults_missing_warnings_to_empty() {
    let result: AskResult = serde_json::from_value(serde_json::json!({
        "query": "what is axon?",
        "answer": "A crawler.",
        "diagnostics": null,
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 3,
            "total": 6
        }
    }))
    .expect("ask result should deserialize without warnings for back-compat");

    assert!(result.warnings.is_empty());
}

#[test]
fn ask_result_omits_empty_warnings_when_serialized() {
    let result = AskResult {
        query: "what is axon?".to_string(),
        answer: "A crawler.".to_string(),
        citation_validation: None,
        session: None,
        warnings: Vec::new(),
        diagnostics: None,
        explain: None,
        timing_ms: axon_api::AskTiming {
            retrieval: 1,
            context_build: 2,
            llm: 3,
            total: 6,
            tei_embed_ms: None,
            qdrant_primary_ms: None,
            qdrant_secondary_ms: None,
            rerank_ms: None,
            top_select_ms: None,
            full_doc_fetch_ms: None,
            supplemental_ms: None,
            llm_ttft_ms: None,
            llm_total_ms: None,
            streamed: None,
            normalize_ms: None,
        },
    };

    let json = serde_json::to_value(&result).expect("ask result should serialize");

    assert!(json.get("warnings").is_none());
}

#[test]
fn ask_result_preserves_retrieval_warnings_without_diagnostics() {
    let result: AskResult = serde_json::from_value(serde_json::json!({
        "query": "what is axon?",
        "answer": "A crawler.",
        "warnings": [
            "ask: keyword search failed; continuing with natural-language retrieval only"
        ],
        "diagnostics": null,
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 3,
            "total": 6
        }
    }))
    .expect("ask result should deserialize warnings without diagnostics");

    assert_eq!(result.warnings.len(), 1);
    assert!(result.diagnostics.is_none());
    assert!(result.warnings[0].contains("keyword search failed"));
}

use super::*;
use axon_llm::LlmBackendKind;

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
fn ask_context_with_follow_up_appends_history_to_existing_context() {
    let cfg = Config {
        ask_follow_up_context: Some("Previous Q&A".to_string()),
        ..Config::default()
    };

    let combined = ask_context_with_follow_up(&cfg, "Sources:\n## Top Chunk [S1]: x");

    assert!(combined.starts_with("Sources:\n## Top Chunk [S1]: x"));
    assert!(combined.ends_with("Previous Q&A"));
}

#[test]
fn ask_context_with_follow_up_seeds_sources_header_when_context_empty() {
    let cfg = Config {
        ask_follow_up_context: Some("Previous Q&A".to_string()),
        ..Config::default()
    };

    let combined = ask_context_with_follow_up(&cfg, "");

    assert_eq!(combined, "Sources:\nPrevious Q&A");
}

#[test]
fn ask_context_with_follow_up_passes_through_when_no_history() {
    let cfg = Config::default();
    let combined = ask_context_with_follow_up(&cfg, "Sources:\n## Top Chunk [S1]: x");
    assert_eq!(combined, "Sources:\n## Top Chunk [S1]: x");
}

#[test]
fn normalized_stream_correction_labels_stored_normalized_answer() {
    let rendered = normalized_stream_correction_text(
        "Answer with normalized citations [S1].\n\n## Sources\n- [S1] https://docs.example.com",
    );

    assert!(rendered.contains("Normalized answer (stored for JSON and follow-up sessions):"));
    assert!(rendered.contains("Answer with normalized citations [S1]."));
    assert!(rendered.starts_with("\n\n---\n\n"));
}

#[test]
fn citation_repair_query_exposes_distinct_document_groups_after_one_source_answer() {
    let mut cfg = Config::test_default();
    cfg.ask_min_citations_nontrivial = 2;
    let query = "Explain semantic decoding, retrieval redaction, source ranges, and preparation concurrency";
    let context = "Sources:\n\
## Top Chunk [S1]: session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa\n\nfirst chunk from document A\n\n---\n\n\
## Top Chunk [S2]: session://codex/doc_session_bbbbbbbbbbbbbbbbbbbbbbbb\n\nchunk from document B\n\n---\n\n\
## Top Chunk [S3]: session://codex/doc_session_aaaaaaaaaaaaaaaaaaaaaaaa\n\nsecond chunk from document A";
    let invalid_answer = normalize_ask_answer(
        &cfg,
        query,
        "The repair decoded semantic content [S1] and made source ranges linear [S3].",
        context,
    );
    let validation = summarize_citation_validation(&invalid_answer);

    assert!(!validation.valid);
    assert_eq!(validation.canonical_citation_count, 1);

    let repair_query = build_citation_repair_query(query, context, &invalid_answer, &validation);

    assert!(repair_query.contains("Document group D1: [S1], [S3]"));
    assert!(repair_query.contains("Document group D2: [S2]"));
    assert!(
        repair_query
            .contains("Citations within one document group do not count as distinct sources")
    );
    assert!(repair_query.contains("cite evidence from at least two different document groups"));
}

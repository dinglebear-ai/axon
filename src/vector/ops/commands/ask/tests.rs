use super::normalize::{extract_cited_source_ids, normalize_ask_answer, parse_context_source_map};
use super::validate_ask_llm_config;
use crate::core::config::{AskBackend, Config};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

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
    assert!(normalized.starts_with("Insufficient evidence in indexed sources"));
    assert!(normalized.contains("## Why"));
    assert!(normalized.contains("## Next Index Targets"));
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
    assert!(normalized.starts_with("Insufficient evidence in indexed sources"));
    assert!(normalized.contains("requires at least 2 unique citations"));
}

#[test]
fn validate_ask_llm_config_accepts_acp_only_config_without_openai_settings() {
    let mut cfg = Config::test_default();
    cfg.openai_base_url.clear();
    cfg.openai_model.clear();
    cfg.acp_adapter_cmd = Some("codex".to_string());

    let result = validate_ask_llm_config(&cfg);

    assert!(result.is_ok(), "ACP-only config should pass validation");
}

#[test]
fn validate_ask_llm_config_rejects_missing_acp_adapter_command() {
    let mut cfg = Config::test_default();
    cfg.openai_base_url.clear();
    cfg.openai_model.clear();
    cfg.acp_adapter_cmd = None;
    cfg.ask_backend = AskBackend::Acp;

    let err = validate_ask_llm_config(&cfg).expect_err("missing adapter should fail");

    assert!(
        err.to_string().contains("AXON_ASK_AGENT"),
        "error should mention AXON_ASK_AGENT: {err}"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn validate_ask_llm_config_accepts_claude_headless_without_acp_adapter() {
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");
    let saved = std::env::var("AXON_ASK_AGENT").ok();
    unsafe { std::env::set_var("AXON_ASK_AGENT", "claude") };
    let mut cfg = Config::test_default();
    cfg.ask_backend = AskBackend::Headless;
    cfg.acp_adapter_cmd = None;

    let result = validate_ask_llm_config(&cfg);
    unsafe {
        match saved {
            Some(v) => std::env::set_var("AXON_ASK_AGENT", v),
            None => std::env::remove_var("AXON_ASK_AGENT"),
        }
    }

    assert!(
        result.is_ok(),
        "Claude headless should not require ACP adapter"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn validate_ask_llm_config_accepts_gemini_headless_without_acp_adapter() {
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");
    let saved = std::env::var("AXON_ASK_AGENT").ok();
    unsafe { std::env::set_var("AXON_ASK_AGENT", "gemini") };
    let mut cfg = Config::test_default();
    cfg.ask_backend = AskBackend::Headless;
    cfg.acp_adapter_cmd = None;

    let result = validate_ask_llm_config(&cfg);
    unsafe {
        match saved {
            Some(v) => std::env::set_var("AXON_ASK_AGENT", v),
            None => std::env::remove_var("AXON_ASK_AGENT"),
        }
    }

    assert!(
        result.is_ok(),
        "Gemini headless should not require ACP adapter"
    );
}

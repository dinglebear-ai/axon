use super::*;
use crate::core::config::types::Config;
use crate::services::llm_backend::LlmBackendKind;

fn cfg_with(backend: LlmBackendKind, openai_model: &str) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.llm_backend = backend;
    cfg.openai_model = openai_model.to_string();
    cfg
}

#[test]
fn gemini_headless_backend_gets_one_million_chars() {
    let cfg = cfg_with(LlmBackendKind::GeminiHeadless, "");
    assert_eq!(model_context_char_budget(&cfg), 1_000_000);
}

#[test]
fn openai_compat_gemini_model_gets_one_million_chars() {
    let cfg = cfg_with(LlmBackendKind::OpenAiCompat, "gemini-2.5-pro");
    assert_eq!(model_context_char_budget(&cfg), 1_000_000);
}

#[test]
fn claude_model_gets_one_million_chars() {
    let cfg = cfg_with(LlmBackendKind::OpenAiCompat, "claude-opus-4-8");
    assert_eq!(model_context_char_budget(&cfg), 1_000_000);
}

#[test]
fn codex_model_gets_four_hundred_thousand_chars() {
    let cfg = cfg_with(LlmBackendKind::OpenAiCompat, "gpt-5-codex");
    assert_eq!(model_context_char_budget(&cfg), 400_000);
}

#[test]
fn unknown_model_assumes_small_window() {
    let cfg = cfg_with(LlmBackendKind::OpenAiCompat, "llama-3.1-8b-instruct");
    assert_eq!(model_context_char_budget(&cfg), 40_000);
}

#[test]
fn codex_backend_gets_four_hundred_thousand_chars_on_default_model() {
    // The codex backend's model lives in `codex_model`, not `openai_model`, and
    // is empty on the codex default — the tier must key off the backend enum.
    let mut cfg = Config::default_minimal();
    cfg.llm_backend = LlmBackendKind::CodexAppServer;
    cfg.openai_model = String::new();
    cfg.codex_model = String::new();
    assert_eq!(ask_model_tier(&cfg), AskModelTier::Medium);
    assert_eq!(model_context_char_budget(&cfg), 400_000);
}

#[test]
fn codex_backend_reads_codex_model_field() {
    let mut cfg = Config::default_minimal();
    cfg.llm_backend = LlmBackendKind::CodexAppServer;
    cfg.codex_model = "gpt-5.5".to_string();
    assert_eq!(ask_model_tier(&cfg), AskModelTier::Medium);
}

#[test]
fn chunk_limit_scales_with_tier() {
    assert_eq!(
        model_chunk_limit(&cfg_with(LlmBackendKind::GeminiHeadless, "")),
        50
    );
    assert_eq!(
        model_chunk_limit(&cfg_with(LlmBackendKind::OpenAiCompat, "gpt-5-codex")),
        28
    );
    assert_eq!(
        model_chunk_limit(&cfg_with(LlmBackendKind::OpenAiCompat, "llama-3.1-8b")),
        10
    );
}

#[test]
fn candidate_pool_scales_with_tier() {
    assert_eq!(
        model_candidate_limit(&cfg_with(LlmBackendKind::GeminiHeadless, "")),
        250
    );
    assert_eq!(
        model_candidate_limit(&cfg_with(LlmBackendKind::OpenAiCompat, "gpt-5-codex")),
        150
    );
    assert_eq!(
        model_candidate_limit(&cfg_with(LlmBackendKind::OpenAiCompat, "llama-3.1-8b")),
        60
    );
}

#[test]
fn model_tier_classifies_known_families() {
    assert_eq!(
        ask_model_tier(&cfg_with(LlmBackendKind::OpenAiCompat, "claude-opus-4-8")),
        AskModelTier::Large
    );
    assert_eq!(
        ask_model_tier(&cfg_with(LlmBackendKind::OpenAiCompat, "gpt-5-codex")),
        AskModelTier::Medium
    );
    assert_eq!(
        ask_model_tier(&cfg_with(LlmBackendKind::OpenAiCompat, "mistral-large")),
        AskModelTier::Small
    );
}

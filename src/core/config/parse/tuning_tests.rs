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

use super::openai_diagnostics_enabled;
use crate::core::config::Config;

#[test]
fn openai_diagnostics_are_disabled_without_openai_base_url() {
    let cfg = Config {
        headless_gemini_cmd: "gemini".to_string(),
        headless_gemini_model: "gemini-3.1-pro-preview".to_string(),
        ..Default::default()
    };

    assert!(!openai_diagnostics_enabled(&cfg, &cfg.openai_model));
}

#[test]
fn openai_diagnostics_are_disabled_for_partial_openai_config() {
    let cfg = Config {
        openai_base_url: "http://localhost:11434/v1".to_string(),
        ..Default::default()
    };

    assert!(!openai_diagnostics_enabled(&cfg, &cfg.openai_model));
}

#[test]
fn openai_diagnostics_are_reported_for_openai_compatible_base_url() {
    let cfg = Config {
        openai_base_url: "http://localhost:11434/v1".to_string(),
        openai_model: "llama3.2".to_string(),
        ..Default::default()
    };

    assert!(openai_diagnostics_enabled(&cfg, &cfg.openai_model));
}

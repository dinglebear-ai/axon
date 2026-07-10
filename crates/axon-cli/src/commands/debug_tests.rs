use super::*;

// gitleaks:allow -- synthetic test fixture, not a real credential
const FAKE_OPENAI_KEY: &str = "sk-abcdefghijklmnopqrstuvwxyz012345";

#[test]
fn llm_debug_analysis_redacts_embedded_secret_before_display() {
    let raw = format!("The configured key {FAKE_OPENAI_KEY} appears to be invalid.");

    let redacted = redact_secrets(&raw);

    assert!(
        !redacted.contains(FAKE_OPENAI_KEY),
        "secret leaked through LLM debug analysis: {redacted}"
    );
    assert!(redacted.contains("[REDACTED]"));
}

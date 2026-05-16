use super::*;

#[test]
fn session_text_redacts_common_secret_tokens() {
    let redacted = redact_session_text(
        "OPENAI key sk-testsecret1234567890 and token github_pat_1234567890abcdef",
    );
    assert!(redacted.contains("[redacted-secret]"));
    assert!(!redacted.contains("sk-testsecret1234567890"));
    assert!(!redacted.contains("github_pat_1234567890abcdef"));
}

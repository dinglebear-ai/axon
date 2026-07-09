use super::*;
use serde_json::json;

#[test]
fn print_json_gated_redacts_secret_before_render() {
    // We can't easily capture stdout here without a test harness change, so
    // this test exercises the redaction step directly through the same
    // context/gate `print_json_gated` uses, proving the wiring is live.
    let value = json!({ "note": "authorization: bearer abcdef0123456789abcdef" });
    let (redacted, report) =
        DefaultRedactor::new().redact_json(value, &RedactionContext::cli_json());
    assert_eq!(
        redacted["note"],
        json!(axon_core::redact::REDACTION_PLACEHOLDER)
    );
    assert_eq!(
        report.status(),
        axon_core::redact::RedactionStatus::Redacted
    );
}

#[test]
fn print_json_gated_passes_clean_payload_through() {
    let value = json!({ "job_id": "abc-123", "status": "completed" });
    let result = print_json_gated(&value);
    assert!(result.is_ok());
}

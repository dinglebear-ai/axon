use super::*;

#[test]
fn redacts_authorization_header_and_bearer_secret() {
    let (out, changed) =
        redact_mcp_output(r#"{"headers":{"authorization":"Bearer secret"},"body":"ok"}"#);
    assert!(changed);
    assert!(!out.to_ascii_lowercase().contains("authorization"));
    assert!(!out.contains("Bearer secret"));
    assert!(out.contains("ok"));
}

#[test]
fn leaves_clean_payload_untouched() {
    let (out, changed) = redact_mcp_output(r#"{"body":"ok"}"#);
    assert_eq!(out, r#"{"body":"ok"}"#);
    assert!(!changed);
}

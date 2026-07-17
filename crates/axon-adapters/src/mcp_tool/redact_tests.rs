use super::*;

#[test]
fn redacts_authorization_header_and_bearer_secret() {
    let (out, changed) =
        redact_mcp_output(r#"{"headers":{"authorization":"Bearer secret"},"body":"ok"}"#);
    assert!(changed);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["headers"]["authorization"],
        "[redacted-secret]"
    );
    assert!(!out.contains("Bearer secret"));
    assert!(out.contains("ok"));
}

#[test]
fn recursively_redacts_secret_bearing_structured_fields() {
    let (out, changed) =
        redact_mcp_output(r#"{"result":{"items":[{"token":"plain-value"}],"password":"hunter2"}}"#);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(changed);
    assert_eq!(parsed["result"]["items"][0]["token"], "[redacted-secret]");
    assert_eq!(parsed["result"]["password"], "[redacted-secret]");
    assert!(!out.contains("plain-value"));
    assert!(!out.contains("hunter2"));
}

#[test]
fn leaves_clean_payload_untouched() {
    let (out, changed) = redact_mcp_output(r#"{"body":"ok"}"#);
    assert_eq!(out, r#"{"body":"ok"}"#);
    assert!(!changed);
}

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
fn redacts_camel_case_secret_fields_but_preserves_benign_security_metadata() {
    let input =
        r#"{"accessToken":"must-not-survive","tokenCount":4096,"authorizationStatus":"required"}"#;
    let (out, changed) = redact_mcp_output(input);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert!(changed);
    assert_eq!(parsed["accessToken"], "[redacted-secret]");
    assert_eq!(parsed["tokenCount"], 4096);
    assert_eq!(parsed["authorizationStatus"], "required");
}

#[test]
fn preserves_low_confidence_tutorial_body_syntax() {
    let input = r#"{"body":"Authorization: Bearer abc123\nTOKEN=abc123\npasswd=hunter2\npostgres://user:password@localhost/app"}"#;
    let (out, changed) = redact_mcp_output(input);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        parsed["body"],
        serde_json::json!(
            "Authorization: Bearer abc123\nTOKEN=abc123\npasswd=hunter2\npostgres://user:password@localhost/app"
        )
    );
    assert!(!changed);
}

#[test]
fn leaves_clean_payload_untouched() {
    let (out, changed) = redact_mcp_output(r#"{"body":"ok"}"#);
    assert_eq!(out, r#"{"body":"ok"}"#);
    assert!(!changed);
}

#[test]
fn preserves_authentication_documentation_and_benign_configuration() {
    let input = r#"{"body":"Bearer authentication uses an Authorization header.\nPORT=3000"}"#;
    let (out, changed) = redact_mcp_output(input);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["body"],
        serde_json::json!("Bearer authentication uses an Authorization header.\nPORT=3000")
    );
    assert!(!changed);
}

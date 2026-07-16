use super::*;

#[test]
fn flags_sensitive_identifier_without_redaction() {
    let source = r#"
fn run() {
    tracing::warn!(token = %token, "provider failed");
}
"#;

    let findings = find_unredacted_logging_calls(Path::new("src/lib.rs"), source);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].line, 3);
    assert_eq!(findings[0].identifier, "token");
}

#[test]
fn allows_sensitive_identifier_when_redacted() {
    let source = r#"
fn run() {
    tracing::warn!(token = %redact_secrets(token), "provider failed");
}
"#;

    let findings = find_unredacted_logging_calls(Path::new("src/lib.rs"), source);

    assert!(findings.is_empty());
}

#[test]
fn scans_multiline_log_helper_calls() {
    let source = r#"
fn run() {
    log_warn(&format!(
        "provider token failed: {token}"
    ));
}
"#;

    let findings = find_unredacted_logging_calls(Path::new("src/lib.rs"), source);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].identifier, "token");
}

#[test]
fn ignores_non_sensitive_log_calls() {
    let source = r#"
fn run() {
    tracing::info!(count = 3, "source-watch scheduler reclaimed stale leases");
}
"#;

    let findings = find_unredacted_logging_calls(Path::new("src/lib.rs"), source);

    assert!(findings.is_empty());
}

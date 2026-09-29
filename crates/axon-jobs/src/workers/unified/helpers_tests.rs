use super::*;

#[test]
fn detached_failure_preserves_safe_item_cause_and_retryability() {
    for retryable in [true, false] {
        let mut error = ApiError::new(
            "adapter.git.read_failed",
            ErrorStage::Fetching,
            "git source indexing failed: read failed",
        )
        .with_source_item_key("src/file.rs")
        .with_context("cause", "git source indexing failed: read failed");
        error.retryable = retryable;
        error.provider_id = Some("filesystem".into());
        let projected = source_error_from_api(&error, Severity::Failed);
        assert_eq!(projected.code, error.code.to_string());
        assert_eq!(projected.retryable, retryable);
        assert_eq!(projected.source_item_key.unwrap().0, "src/file.rs");
        assert_eq!(projected.provider_id.unwrap().0, "filesystem");
        assert_eq!(projected.cause.as_deref(), Some(error.message.as_str()));
    }
}

#[test]
fn detached_failure_sanitizes_display_fields_before_status_persistence() {
    let secret = format!("sk-{}", "a".repeat(32));
    let text = format!(
        "authorization: Bearer {secret}; /tmp/axon-secret/checkout.rs; https://user:private-password@example.test/path\r\n"
    );
    let error = ApiError::new("adapter.read_failed", ErrorStage::Fetching, &text)
        .with_source_item_key(format!("src/{secret}\r\n{}", "é".repeat(1000)))
        .with_context("cause", text);
    let projected = source_error_from_api(&error, Severity::Failed);
    let serialized = serde_json::to_string(&projected).unwrap();
    for hidden in [&secret, "private-password", "/tmp/axon-secret/checkout.rs"] {
        assert!(!serialized.contains(hidden));
    }
    let item = projected.source_item_key.unwrap().0;
    assert!(item.len() <= 512);
    assert!(!item.chars().any(char::is_control));
}

#[test]
fn detached_projection_is_idempotent_for_sanitized_url_parameters() {
    let raw = "HTTPS://user:password@example.test/repo?token=short-secret#fragment next";
    let safe = public_diagnostic(raw, 4096);
    assert!(!safe.contains("short-secret"));
    assert!(!safe.contains("password"));
    assert!(!safe.contains("fragment"));
    assert_eq!(public_diagnostic(&safe, 4096), safe);
}

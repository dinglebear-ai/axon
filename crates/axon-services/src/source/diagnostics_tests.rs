use super::*;

#[test]
fn typed_acquisition_error_keeps_code_stage_identity_and_explicit_retry_policy() {
    for retryable in [false, true] {
        let mut cause = ApiError::new(
            "adapter.git.read_failed",
            ErrorStage::Fetching,
            "failed to open repository item",
        )
        .with_context("source_item_key", "src/file.rs")
        .with_context("io_kind", "PermissionDenied")
        .with_context("body", "untrusted body must not be projected");
        cause.retryable = retryable;
        cause.provider_id = Some("file-provider".into());
        cause.retry_after_ms = Some(321);
        let original = cause.clone();
        let error = anyhow::Error::new(cause).context("git source indexing failed");
        let projected = api_error(&error);
        let terminal = source_error(&error);
        assert_eq!(projected.code, original.code);
        assert_eq!(projected.stage, ErrorStage::Fetching);
        assert_eq!(projected.retryable, retryable);
        assert_eq!(projected.retry_after_ms, Some(321));
        assert_eq!(projected.provider_id.as_deref(), Some("file-provider"));
        assert_eq!(projected.source_item_key.as_deref(), Some("src/file.rs"));
        assert_eq!(projected.details["io_kind"], "PermissionDenied");
        assert!(!projected.details.contains_key("body"));
        assert_eq!(terminal.code, projected.code.to_string());
        assert_eq!(terminal.message, projected.message);
        assert_eq!(terminal.message.matches("(Fetching)").count(), 1);
        assert_eq!(terminal.retryable, projected.retryable);
        assert_eq!(
            terminal.source_item_key.unwrap().0,
            projected.source_item_key.unwrap()
        );
        assert_eq!(terminal.cause, projected.details.get("cause").cloned());
        assert_eq!(error.downcast_ref::<ApiError>(), Some(&original));
    }
}

#[test]
fn projections_scrub_secrets_paths_controls_and_bound_utf8_without_changing_identity() {
    let secret = format!("sk-{}", "a".repeat(32));
    let identity = format!("src/{secret}\r\n{}.rs", "é".repeat(800));
    let mut root = ApiError::new("adapter.git.read_failed", ErrorStage::Fetching,
        format!("authorization: Bearer {secret}; /tmp/axon-checkout/private.rs; https://login:private-password@example.test/repo?token=synthetic-token"))
        .with_source_item_key(identity.clone());
    root.details.insert("body".into(), "PRIVATE-BODY".into());
    let error = anyhow::Error::new(root).context(format!("failed\r\ncontext {secret}"));
    let api = api_error(&error);
    let wire = serde_json::to_string(&api).unwrap();
    for hidden in [
        &secret,
        "private-password",
        "PRIVATE-BODY",
        "synthetic-token",
        "/tmp/axon-checkout/private.rs",
    ] {
        assert!(!wire.contains(hidden), "leaked {hidden}");
    }
    assert!(api.message.len() <= MESSAGE_BYTES);
    let item = api.source_item_key.unwrap();
    assert!(item.len() <= IDENTITY_BYTES);
    assert!(!item.chars().any(char::is_control));
    assert!(!api.message.chars().any(char::is_control));
    assert_eq!(
        error
            .downcast_ref::<ApiError>()
            .unwrap()
            .source_item_key
            .as_deref(),
        Some(identity.as_str())
    );
}

#[test]
fn typed_item_field_wins_over_legacy_detail_and_oversized_diagnostics_fail_closed() {
    let error = anyhow::Error::new(
        ApiError::new("adapter.read_failed", ErrorStage::Fetching, "read failed")
            .with_source_item_key("correct/file")
            .with_context("source_item_key", "wrong/file"),
    );
    assert_eq!(
        api_error(&error).source_item_key.as_deref(),
        Some("correct/file")
    );
    let oversized = "x".repeat(axon_core::redact::MAX_REDACTABLE_TEXT_BYTES + 1);
    assert_eq!(
        display_text(&oversized, 512),
        "diagnostic detail suppressed"
    );
    assert_eq!(display_text(&"é".repeat(400), 511).len(), 510);
}

#[test]
fn display_projection_strips_url_parameters_and_is_idempotent() {
    let text = "HTTPS://user:password@example.test/repo?token=short-secret#fragment next";
    let safe = display_text(text, 4096);
    assert!(!safe.contains("short-secret"));
    assert!(!safe.contains("password"));
    assert!(!safe.contains("fragment"));
    assert_eq!(display_text(&safe, 4096), safe);
}

#[test]
fn diagnostic_uses_earliest_url_when_schemes_share_a_token() {
    let safe = display_text("http://a/?token=short-secret,https://b/", 4096);
    assert_eq!(safe, "http://a/[REDACTED]");
    assert!(!safe.contains("short-secret"));
}

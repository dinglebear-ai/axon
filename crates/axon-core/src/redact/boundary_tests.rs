use super::*;
use axon_api::source::ErrorStage;
use serde_json::json;

fn ctx() -> RedactionContext {
    RedactionContext::vector_payload(Some(SourceKind::Web))
}

#[test]
fn clean_payload_reports_clean() {
    let value = json!({
        "web_title": "Hello world",
        "content_kind": "markdown",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value.clone(), &ctx());
    assert_eq!(out, value);
    assert_eq!(report.status(), RedactionStatus::Clean);
    assert!(!report.status_redacted);
    assert!(report.redacted_fields.is_empty());
    assert!(report.dropped_fields.is_empty());
}

#[test]
fn secret_value_is_scrubbed_and_reported_redacted() {
    let value = json!({
        "note": "authorization: bearer abcdef0123456789abcdef",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out["note"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(report.status(), RedactionStatus::Redacted);
    assert!(report.redacted_fields.contains(&"note".to_string()));
    assert!(
        report
            .detectors_triggered
            .contains(&"secret_value".to_string())
    );
}

#[test]
fn secret_named_field_is_dropped() {
    let value = json!({
        "access_token": "value-that-is-not-obviously-secret",
        "web_title": "keep me",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert!(out.get("access_token").is_none());
    assert_eq!(out["web_title"], json!("keep me"));
    assert_eq!(report.status(), RedactionStatus::Redacted);
    assert!(report.dropped_fields.contains(&"access_token".to_string()));
    assert!(
        report
            .detectors_triggered
            .contains(&"secret_field_name".to_string())
    );
}

#[test]
fn chunk_text_body_is_not_masked_by_redactor() {
    // The redactor must not launder a secret in the retrievable body — that is
    // the hard-skip validator's job. chunk_text passes through unchanged.
    let secret_body = "here is a token=deadbeefdeadbeefdeadbeef in the text";
    let value = json!({ "chunk_text": secret_body });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out["chunk_text"], json!(secret_body));
    assert_eq!(report.status(), RedactionStatus::Clean);
}

#[test]
fn structural_status_fields_are_preserved() {
    let value = json!({
        "redaction_status": "clean",
        "visibility": "internal",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value.clone(), &ctx());
    assert_eq!(out, value);
    assert_eq!(report.status(), RedactionStatus::Clean);
}

#[test]
fn deterministic_same_input_same_output() {
    let value = json!({
        "a": "authorization: bearer 0123456789abcdef0123",
        "client_secret": "x",
        "web_title": "ok",
    });
    let (out1, r1) = DefaultRedactor::new().redact_json(value.clone(), &ctx());
    let (out2, r2) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out1, out2);
    assert_eq!(r1, r2);
}

#[test]
fn nested_secret_value_is_scrubbed_with_path() {
    let value = json!({
        "meta": { "detail": "api_key=abcdef0123456789abcdef" },
    });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out["meta"]["detail"], json!(REDACTION_PLACEHOLDER));
    assert!(report.redacted_fields.contains(&"meta.detail".to_string()));
}

#[test]
fn classify_field_bands() {
    let redactor = DefaultRedactor::new();
    assert_eq!(
        redactor.classify_field("client_secret", &json!("x")),
        Visibility::Sensitive
    );
    assert_eq!(
        redactor.classify_field("web_title", &json!("hi")),
        Visibility::Internal
    );
    assert_eq!(
        redactor.classify_field("note", &json!("authorization: bearer abcdef0123456789abcd")),
        Visibility::Sensitive
    );
    assert_eq!(
        redactor.classify_field("redaction_status", &json!("clean")),
        Visibility::Internal
    );
}

#[test]
fn unknown_adapter_metadata_defaults_to_internal() {
    let redactor = DefaultRedactor::new();

    assert_eq!(
        redactor.classify_field("adapter_blob", &json!({ "raw": "not classified" })),
        Visibility::Internal
    );
}

#[test]
fn redact_text_scrubs_secrets_and_local_paths() {
    let r = DefaultRedactor::new();
    let c = ctx();
    assert_eq!(
        r.redact_text("authorization: bearer abcdef0123456789abcd", &c),
        REDACTION_PLACEHOLDER
    );
    assert_eq!(
        r.redact_text("see /home/jmagar/secret.rs for details", &c),
        REDACTION_PLACEHOLDER
    );
    assert_eq!(r.redact_text("just normal prose", &c), "just normal prose");

    // When the surface allows internal paths, the path is preserved (but a real
    // secret is still scrubbed).
    let allow = RedactionContext {
        allow_internal_paths: true,
        ..ctx()
    };
    assert_eq!(
        r.redact_text("see /home/jmagar/notes.md", &allow),
        "see /home/jmagar/notes.md"
    );
}

#[test]
fn structured_event_text_scrubs_local_paths_unless_explicitly_allowed() {
    let value = json!({"message": "read /home/jmagar/.ssh/id_ed25519 next"});
    let (redacted, report) =
        DefaultRedactor::new().redact_json(value.clone(), &RedactionContext::job_event());
    assert_eq!(redacted["message"], REDACTION_PLACEHOLDER);
    assert_eq!(report.detectors_triggered, ["local_path"]);

    let mut allowed = RedactionContext::job_event();
    allowed.allow_internal_paths = true;
    let (preserved, report) = DefaultRedactor::new().redact_json(value.clone(), &allowed);
    assert_eq!(preserved, value);
    assert_eq!(report.status(), RedactionStatus::Clean);
}

#[test]
fn cli_json_output_secret_fixture_fails_before_render() {
    // Fail-closed contract for the CLI JSON surface: a secret-bearing result
    // payload must be scrubbed (never passed through unmodified) before it
    // reaches stdout via `--json`. This mirrors the vector-payload fixture
    // shape/assertion style above, adapted to this crate's real gate API
    // (`Redactor::redact_json` returning `(Value, RedactionReport)`, not a
    // `Result<_, ApiError>` — there is no `redact_public_write` function in
    // this codebase).
    let payload = json!({
        "job_id": "abc-123",
        "detail": "authorization: bearer abcdef0123456789abcdef",
    });
    let context = RedactionContext::cli_json();
    assert_eq!(context.surface, RedactionSurface::CliJson);
    let (out, report) = DefaultRedactor::new().redact_json(payload, &context);
    assert_eq!(out["detail"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(report.status(), RedactionStatus::Redacted);
}

#[test]
fn artifact_metadata_secret_fixture_fails_before_write() {
    // Fail-closed contract for the artifact-metadata surface: a secret-bearing
    // artifact payload (e.g. a watch `url-change` summary embedding a leaked
    // token) must be scrubbed before the row is persisted. Real gate API is
    // `Redactor::redact_json` returning `(Value, RedactionReport)`; there is
    // no `redact_public_write`/`ApiError`-shaped function in this codebase.
    let metadata = json!({
        "url": "https://example.com/a",
        "summary": "rotated authorization: bearer abcdef0123456789abcdef",
    });
    let context = RedactionContext::artifact_metadata();
    assert_eq!(context.surface, RedactionSurface::Artifacts);
    let (out, report) = DefaultRedactor::new().redact_json(metadata, &context);
    assert_eq!(out["summary"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(report.status(), RedactionStatus::Redacted);
}

#[test]
fn opaque_gitlab_token_is_scrubbed_by_context_plus_entropy() {
    // Gitea/GitLab/OAuth-style opaque tokens have no fixed prefix, so the
    // structured boundary must classify them by field-name context plus
    // value entropy.
    // Field name uses "gitlab" (opaque-token context) rather than "token"/
    // "secret"/etc. — those field-name fragments already get dropped
    // non-fatally by the existing key-name detector before this value ever
    // reaches the entropy classifier, so this fixture exercises the new
    // context-plus-entropy path specifically.
    let value = json!({
        "gitlab_identifier": "aK9fQ2mP7zT4xL8vN1cR6bY3wE0sJ5h",
        "web_title": "keep me",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out["gitlab_identifier"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(out["web_title"], json!("keep me"));
    assert!(
        report
            .redacted_fields
            .contains(&"gitlab_identifier".to_string())
    );
    assert!(
        report
            .detectors_triggered
            .contains(&"opaque_token_entropy".to_string())
    );
}

#[test]
fn low_entropy_value_in_token_context_field_is_untouched() {
    // A non-secret lookalike: field name matches the opaque-token context,
    // but the value doesn't look random (too short / low entropy), so it
    // must not be flagged as a false positive.
    let value = json!({ "gitlab_identifier": "not-a-real-secret" });
    let (out, report) = DefaultRedactor::new().redact_json(value.clone(), &ctx());
    assert_eq!(out, value);
    assert_eq!(report.status(), RedactionStatus::Clean);
}

#[test]
fn pem_private_key_and_url_credentials_are_scrubbed_in_free_text_field() {
    let value = json!({
        "note": "-----BEGIN RSA PRIVATE KEY-----\nMIIB...\n-----END RSA PRIVATE KEY-----",
        "other": "connect via postgres://myuser:s3cr3tpass@db.internal:5432/mydb",
    });
    let (out, report) = DefaultRedactor::new().redact_json(value, &ctx());
    assert_eq!(out["note"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(out["other"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(report.status(), RedactionStatus::Redacted);
}

#[test]
fn redact_metadata_roundtrips_map() {
    let mut map = MetadataMap::default();
    map.insert("web_title".to_string(), json!("keep"));
    // `access_token` is secret-like (dropped) but not a hard *forbidden* field
    // name (which the validator rejects fatally instead).
    map.insert("access_token".to_string(), json!("drop-me"));
    let (out, report) = redact_metadata(map, &ctx(), &DefaultRedactor::new());
    assert!(out.get("web_title").is_some());
    assert!(out.get("access_token").is_none());
    assert_eq!(report.status(), RedactionStatus::Redacted);
}

#[test]
fn stamp_redaction_metadata_records_status_version_and_counts() {
    let mut map = MetadataMap::default();
    map.insert(
        "note".to_string(),
        json!("authorization: bearer abcdef0123456789abcdef"),
    );

    let (map, report) = redact_metadata(map, &ctx(), &DefaultRedactor::new());
    let stamped = stamp_redaction_metadata(map, &report);

    assert_eq!(stamped["redaction_status"], json!("redacted"));
    assert_eq!(stamped["redaction_version"], json!(REDACTION_VERSION));
    assert_eq!(stamped["redacted_field_count"], json!(1));
    assert_eq!(stamped["dropped_field_count"], json!(0));
    assert_eq!(stamped["detector_count"], json!(1));
    assert_eq!(stamped["detector_names"], json!(["secret_value"]));
    assert_eq!(stamped["visibility"], json!("internal"));
}

#[test]
fn public_write_rejects_forbidden_fields_before_returning_payload() {
    let error = redact_public_write(
        json!({"authorization": "Bearer definitely-secret"}),
        &ctx(),
        &DefaultRedactor::new(),
    )
    .expect_err("forbidden auth field must fail closed");

    assert_eq!(error.code.0, "redaction.failed");
    assert_eq!(error.stage, ErrorStage::Authorizing);
}

#[test]
fn public_write_rejects_oversized_text_before_redaction() {
    let error = redact_public_write(
        json!({"message": "x".repeat(MAX_REDACTABLE_TEXT_BYTES + 1)}),
        &ctx(),
        &DefaultRedactor::new(),
    )
    .expect_err("oversized text must fail closed");

    assert_eq!(error.code.0, "redaction.failed");
    assert_eq!(error.stage, ErrorStage::Authorizing);
}

#[test]
fn public_write_returns_bounded_contract_report() {
    let write = redact_public_write(
        json!({"message": "authorization: bearer abcdef0123456789abcdef"}),
        &ctx(),
        &DefaultRedactor::new(),
    )
    .expect("ordinary secret text is scrubbed");

    assert_eq!(write.payload["message"], json!(REDACTION_PLACEHOLDER));
    assert_eq!(write.redaction.redaction_status, RedactionStatus::Redacted);
    assert_eq!(write.redaction.redaction_version, REDACTION_VERSION);
    assert_eq!(write.redaction.redacted_field_count, 1);
    assert_eq!(write.redaction.dropped_field_count, 0);
    assert_eq!(write.redaction.detector_count, 1);
    assert_eq!(write.redaction.detector_names, ["secret_value"]);
}

#[test]
fn every_public_write_surface_produces_redaction_provenance() {
    let surfaces = [
        RedactionSurface::Logs,
        RedactionSurface::JobEvents,
        RedactionSurface::VectorPayload,
        RedactionSurface::GraphEvidence,
        RedactionSurface::MemoryRecords,
        RedactionSurface::Artifacts,
        RedactionSurface::CliJson,
        RedactionSurface::McpResponse,
        RedactionSurface::RestResponse,
    ];

    for surface in surfaces {
        let context = RedactionContext {
            visibility_ceiling: Visibility::Public,
            surface,
            source_kind: None,
            allow_internal_paths: false,
        };
        let write = redact_public_write(
            json!({"message": "authorization: bearer abcdef0123456789abcdef"}),
            &context,
            &DefaultRedactor::new(),
        )
        .expect("surface redacts");

        assert_eq!(write.redaction.redaction_status, RedactionStatus::Redacted);
        assert_eq!(write.redaction.redaction_version, REDACTION_VERSION);
        assert_eq!(write.redaction.visibility, Visibility::Public);
    }
}

#[test]
fn report_field_and_detector_counts_are_bounded() {
    let mut fields = serde_json::Map::new();
    for index in 0..400 {
        fields.insert(
            format!("field_{index}"),
            json!("authorization: bearer abcdef0123456789abcdef"),
        );
        fields.insert(format!("field_{index}_token"), json!("opaque"));
    }

    let write = redact_public_write(
        serde_json::Value::Object(fields),
        &ctx(),
        &DefaultRedactor::new(),
    )
    .expect("bounded report succeeds");

    assert_eq!(write.redaction.redacted_field_count, 256);
    assert_eq!(write.redaction.dropped_field_count, 256);
    assert_eq!(write.redaction.detector_count, 2);
    assert_eq!(write.redaction.detector_names.len(), 2);
}

use super::*;
use crate::context::ErrorVisibility;
use crate::retry::RetryScope;
use crate::severity::ErrorSeverity;
use crate::stage::ErrorStage;

#[test]
fn new_derives_severity_and_retryable_from_code() {
    let err = ApiError::new(
        "provider.unavailable",
        ErrorStage::Embedding,
        "provider down",
    );
    assert_eq!(err.severity, ErrorSeverity::Failed);
    assert!(err.retryable);
    assert_eq!(err.visibility, ErrorVisibility::Public);
}

#[test]
fn builder_sets_correlation_ids_as_strings() {
    let err = ApiError::new("ledger.transaction", ErrorStage::Leasing, "boom")
        .with_source_id("src_1")
        .with_job_id("job_1")
        .with_provider_id("tei")
        .with_context("provider", "tei");
    assert_eq!(err.source_id.as_deref(), Some("src_1"));
    assert_eq!(err.job_id.as_deref(), Some("job_1"));
    assert_eq!(err.provider_id.as_deref(), Some("tei"));
    assert_eq!(err.details.get("provider").map(String::as_str), Some("tei"));
}

#[test]
fn api_error_round_trips_serde_and_omits_none() {
    let err = ApiError::new("command.unknown", ErrorStage::Parsing, "nope");
    let value = serde_json::to_value(&err).unwrap();
    assert_eq!(value["code"], "command.unknown");
    assert_eq!(value["stage"], "parsing");
    assert_eq!(value["severity"], "failed");
    assert_eq!(value["visibility"], "public");
    assert!(value.get("job_id").is_none(), "None ids are skipped");

    let back: ApiError = serde_json::from_value(value).unwrap();
    assert_eq!(back, err);
}

#[test]
fn retry_policy_infers_scope_from_ids() {
    let provider =
        ApiError::new("provider.unavailable", ErrorStage::Embedding, "x").with_provider_id("tei");
    assert_eq!(provider.retry_policy().retry_scope, RetryScope::Provider);

    let job = ApiError::new("ledger.transaction", ErrorStage::Leasing, "x");
    assert_eq!(job.retry_policy().retry_scope, RetryScope::Job);
}

#[test]
fn degradation_policy_follows_severity() {
    let degraded = ApiError::new("parser.fallback", ErrorStage::ParsingContent, "x")
        .with_severity(ErrorSeverity::Degraded);
    assert!(degraded.degradation_policy().degradable);

    let failed = ApiError::new("vector.upsert_failed", ErrorStage::Upserting, "x");
    assert!(!failed.degradation_policy().degradable);
}

#[test]
fn display_never_leaks_details_or_ids() {
    let err = ApiError::new(
        "provider.unavailable",
        ErrorStage::Embedding,
        "provider down",
    )
    .with_visibility(ErrorVisibility::Sensitive)
    .with_source_id("src_secret_local_path")
    .with_context("secret_token", "hunter2");
    let shown = err.to_string();
    assert!(shown.contains("provider.unavailable"));
    assert!(shown.contains("provider down"));
    assert!(
        !shown.contains("hunter2"),
        "details value must not leak via Display: {shown}"
    );
    assert!(
        !shown.contains("src_secret_local_path"),
        "correlation ids must not leak via Display: {shown}"
    );
}

#[test]
fn provider_cooling_present_only_when_cooldown_set() {
    let no_cool = ApiError::new("provider.unavailable", ErrorStage::Embedding, "x");
    assert!(no_cool.provider_cooling().is_none());

    let until = chrono::Utc::now();
    let cooling = ApiError::new("provider.unavailable", ErrorStage::Embedding, "x")
        .with_provider_id("tei")
        .with_cooldown_until(until);
    let cooling = cooling.provider_cooling().unwrap();
    assert_eq!(cooling.provider_id.as_deref(), Some("tei"));
    assert_eq!(cooling.cooldown_until, until);
}

#[test]
fn builders_attach_retry_cooling_and_item_projection() {
    let until = chrono::Utc::now();
    let err = ApiError::new("provider.cooling", ErrorStage::Embedding, "cooling")
        .with_retry_after_ms(30_000)
        .with_provider_cooling(crate::ProviderCooling::new(until).with_provider("tei"))
        .with_context("safe_detail", "redacted");

    assert_eq!(err.retry_after_ms, Some(30_000));
    assert_eq!(err.cooldown_until, Some(until));
    assert_eq!(err.provider_id.as_deref(), Some("tei"));

    let item = err.to_source_item_error("src_1", "item.md", "7", "failed", 2);
    assert_eq!(item.source_id, "src_1");
    assert_eq!(item.source_item_key, "item.md");
    assert_eq!(item.generation, "7");
    assert_eq!(item.error_code.to_string(), "provider.cooling");
    assert_eq!(item.error_stage, ErrorStage::Embedding);
    assert!(item.retryable);
    assert_eq!(item.attempt, 2);
    assert_eq!(item.message, "cooling");
    assert_eq!(item.provider_id.as_deref(), Some("tei"));
    assert_eq!(item.retry_after_ms, Some(30_000));
    assert_eq!(item.cooldown_until, Some(until));
    assert_eq!(
        item.details.get("safe_detail").map(String::as_str),
        Some("redacted")
    );
}

#[test]
fn redaction_failed_constructor_is_fail_closed_public() {
    let err = ApiError::redaction_failed("vector_payload");
    assert_eq!(err.code.to_string(), "redaction.failed");
    assert_eq!(err.stage, ErrorStage::Authorizing);
    assert_eq!(err.severity, ErrorSeverity::Fatal);
    assert_eq!(err.visibility, ErrorVisibility::Public);
    assert!(!err.retryable);
    assert_eq!(
        err.details.get("surface").map(String::as_str),
        Some("vector_payload")
    );
}

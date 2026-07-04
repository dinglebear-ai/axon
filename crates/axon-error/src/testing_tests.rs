use super::*;
use crate::context::ErrorVisibility;
use crate::severity::ErrorSeverity;

#[test]
fn test_error_uses_code_and_stage() {
    let err = test_error("vector.upsert_failed", ErrorStage::Upserting);
    assert_eq!(err.stage, ErrorStage::Upserting);
    assert_eq!(err.code.to_string(), "vector.upsert_failed");
}

#[test]
fn retryable_provider_outage_is_retryable_and_cooling() {
    let err = retryable_provider_outage();
    assert!(err.retryable);
    assert_eq!(err.provider_id.as_deref(), Some("tei"));
    assert!(err.provider_cooling().is_some());
}

#[test]
fn fatal_config_failure_is_fatal_internal() {
    let err = fatal_config_failure();
    assert_eq!(err.severity, ErrorSeverity::Fatal);
    assert!(!err.retryable);
    assert_eq!(err.visibility, ErrorVisibility::Internal);
}

#[test]
fn degraded_parser_is_degraded() {
    let err = degraded_parser();
    assert_eq!(err.severity, ErrorSeverity::Degraded);
    assert!(err.degradation_policy().degradable);
}

#[test]
fn fixtures_round_trip_serde() {
    for err in [
        retryable_provider_outage(),
        fatal_config_failure(),
        degraded_parser(),
    ] {
        let value = serde_json::to_value(&err).unwrap();
        let back: ApiError = serde_json::from_value(value).unwrap();
        assert_eq!(back, err);
    }
}

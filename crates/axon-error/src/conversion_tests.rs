use super::*;
use crate::stage::ErrorStage;

struct FakeProviderError {
    detail: String,
}

impl IntoApiError for FakeProviderError {
    fn into_api_error(self) -> ApiError {
        // Projects a root-cause class to a stable code without leaking the raw
        // provider detail into the public message.
        ApiError::new(
            "provider.unavailable",
            ErrorStage::Embedding,
            "Embedding provider is unavailable.",
        )
        .with_context("root_cause_class", "network")
        .with_context("detail_len", self.detail.len().to_string())
    }
}

#[test]
fn into_api_error_projects_without_exposing_internals() {
    let err = FakeProviderError {
        detail: "tcp connect timeout to 10.0.0.5:443".to_string(),
    };
    let api = project(err);
    assert_eq!(
        api.code,
        crate::code::ErrorCode::from("provider.unavailable")
    );
    assert!(api.retryable);
    assert!(!api.message.contains("10.0.0.5"));
}

#[test]
fn api_error_into_api_error_is_identity() {
    let err = ApiError::new("vector.upsert_failed", ErrorStage::Upserting, "boom");
    assert_eq!(err.clone().into_api_error(), err);
}

#[test]
fn from_parts_derives_classification() {
    let err = api_error_from_parts("provider.unavailable", ErrorStage::Embedding, "down");
    assert!(err.retryable);
}

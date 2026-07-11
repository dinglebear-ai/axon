use super::*;
use axon_api::source::{HealthStatus, LlmCompletionRequest, ProviderKind};

use crate::fake::{FakeLlmMode, FakeLlmProvider};

#[tokio::test]
async fn fake_llm_returns_deterministic_completion_and_records_calls() {
    let provider = FakeLlmProvider::new("fake-llm");
    let request = LlmCompletionRequest::prompt("Summarize Axon");

    let first = provider.complete(request.clone()).await.unwrap();
    let second = provider.complete(request).await.unwrap();

    assert_eq!(first.text, second.text);
    assert_eq!(first.model, "fake-llm");
    assert_eq!(provider.calls().await.len(), 2);
}

#[tokio::test]
async fn fake_llm_streaming_emits_deltas_and_final_response() {
    let provider = FakeLlmProvider::new("fake-llm");
    let mut deltas = Vec::new();

    let response = provider
        .complete_streaming(LlmCompletionRequest::prompt("stream it"), &mut |delta| {
            deltas.push(delta.text);
        })
        .await
        .unwrap();

    assert!(!deltas.is_empty());
    assert_eq!(deltas.join(""), response.text);
}

#[tokio::test]
async fn fake_llm_reports_capabilities_health_and_failure_modes() {
    let provider = FakeLlmProvider::new("fake-llm")
        .with_health(HealthStatus::Unavailable)
        .with_mode(FakeLlmMode::Timeout);

    let capability = provider.capabilities().await.unwrap();
    assert_eq!(capability.provider_kind, ProviderKind::Llm);
    assert_eq!(capability.health, HealthStatus::Unavailable);

    let err = provider
        .complete(LlmCompletionRequest::prompt("timeout"))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.timeout");
    assert!(err.retryable);
}

#[tokio::test]
async fn fake_llm_capabilities_reflect_failure_mode() {
    let timeout = FakeLlmProvider::new("fake-llm").with_mode(FakeLlmMode::Timeout);
    assert_eq!(
        timeout.capabilities().await.unwrap().health,
        HealthStatus::Degraded
    );

    let rate_limited = FakeLlmProvider::new("fake-llm").with_mode(FakeLlmMode::RateLimited);
    let capability = rate_limited.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.rate_limited"
    );

    let fatal = FakeLlmProvider::new("fake-llm").with_mode(FakeLlmMode::Fatal);
    let capability = fatal.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Unavailable);
    let error = capability.last_error.unwrap();
    assert_eq!(error.code.to_string(), "provider.fatal");
    assert_eq!(error.provider_id, Some("fake-llm".to_string()));
    assert!(!error.retryable);

    let err = fatal
        .complete(LlmCompletionRequest::prompt("fatal"))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.fatal");
    assert!(!err.retryable);
}

#[tokio::test]
async fn fake_llm_with_cooldown_until_overrides_the_mode_derived_timestamp() {
    // `with_cooldown_until` lets a test simulate a live, "now"-relative
    // cooldown window instead of `FakeLlmMode::RateLimited`'s fixed timestamp.
    let cooldown_until =
        axon_api::source::Timestamp::from(chrono::Utc::now() + chrono::Duration::seconds(45));
    let provider = FakeLlmProvider::new("fake-llm")
        .with_mode(FakeLlmMode::RateLimited)
        .with_cooldown_until(cooldown_until.clone());

    let capability = provider.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert_eq!(capability.cooldown_until, Some(cooldown_until));
}

#[tokio::test]
async fn fake_llm_health_override_cannot_hide_failure_mode() {
    let provider = FakeLlmProvider::new("fake-llm")
        .with_health(HealthStatus::Healthy)
        .with_mode(FakeLlmMode::Fatal);

    let capability = provider.capabilities().await.unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.fatal"
    );
}

#[tokio::test]
async fn fake_llm_returns_structured_payload_for_schema_requests() {
    let provider = FakeLlmProvider::new("fake-llm");
    let mut request = LlmCompletionRequest::prompt("Return a JSON summary");
    request.response_schema = Some(serde_json::json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string" },
            "checksum": { "type": "number" }
        },
        "required": ["provider", "checksum"]
    }));

    let response = provider.complete(request).await.unwrap();
    let structured = response.structured.unwrap();

    assert_eq!(structured["provider"], "fake-llm");
    assert!(structured["checksum"].is_number());
}

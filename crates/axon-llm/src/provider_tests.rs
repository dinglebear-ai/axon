use axon_api::source::{HealthStatus, LlmCompletionRequest, ProviderKind};

use crate::fake::{FakeLlmMode, FakeLlmProvider};
use crate::provider::LlmProvider;

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

use axon_api::source::{
    HealthStatus, JobId, JobPriority, LifecycleStatus, PipelinePhase, ProviderId, ProviderKind,
    SourceGenerationId, SourceId, SourceItemKey,
};
use axon_error::{ApiError, ErrorSeverity, ErrorStage};

use crate::provider_failure::{ProviderFailureContext, project_provider_failure};
use crate::reservation::{
    ProviderReservationConfig, ProviderReservationManager, ProviderReservationOutcome,
};

fn manager() -> ProviderReservationManager {
    ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("fake-embedding"),
        provider_kind: ProviderKind::Embedding,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 2,
        cooldown_secs: 30,
    })
}

fn context() -> ProviderFailureContext {
    ProviderFailureContext {
        job_id: JobId::new(uuid::Uuid::from_u128(1)),
        stage_id: None,
        phase: PipelinePhase::Embedding,
        source_id: Some(SourceId::from("src_fake")),
        source_item_key: Some(SourceItemKey::from("item.md")),
        generation: Some(SourceGenerationId::from("gen_1")),
        attempt: 2,
        max_attempts: Some(4),
    }
}

#[tokio::test]
async fn fake_provider_cooling_reaches_error_event_and_item_projection() {
    let manager = manager();
    let failure = || {
        ApiError::new(
            "provider.timeout",
            ErrorStage::Embedding,
            "fake provider timed out",
        )
    };

    let first = project_provider_failure(&manager, failure(), context()).await;
    assert_eq!(first.outcome, ProviderReservationOutcome::Recorded);
    assert!(first.error.cooldown_until.is_none());

    let second = project_provider_failure(&manager, failure(), context()).await;
    assert_eq!(second.outcome, ProviderReservationOutcome::Cooling);
    assert_eq!(second.error.provider_id.as_deref(), Some("fake-embedding"));
    assert_eq!(second.error.retry_after_ms, Some(30_000));
    assert!(second.error.cooldown_until.is_some());
    assert_eq!(second.event.status, LifecycleStatus::Waiting);
    assert_eq!(second.event.error.as_ref(), Some(&second.error));
    let expected_retry_at = second
        .error
        .cooldown_until
        .map(axon_api::source::Timestamp::from);
    assert_eq!(
        second
            .event
            .retry
            .as_ref()
            .and_then(|retry| retry.next_retry_at.as_ref()),
        expected_retry_at.as_ref()
    );
    let item = second.item_error.expect("item projection");
    assert_eq!(item.error_code.to_string(), "provider.timeout");
    assert_eq!(item.source_item_key, "item.md");
    assert_eq!(item.provider_id.as_deref(), Some("fake-embedding"));
    assert_eq!(item.retry_after_ms, Some(30_000));
    assert_eq!(item.cooldown_until, second.error.cooldown_until);
    assert!(item.retryable);
    assert_eq!(manager.health().await, HealthStatus::Cooling);
}

#[tokio::test]
async fn redaction_failure_is_fatal_and_does_not_poison_provider_health() {
    let manager = manager();
    let projection = project_provider_failure(
        &manager,
        ApiError::redaction_failed("provider_output"),
        context(),
    )
    .await;

    assert_eq!(projection.error.severity, ErrorSeverity::Fatal);
    assert!(!projection.error.retryable);
    assert!(projection.error.provider_id.is_none());
    assert!(projection.error.cooldown_until.is_none());
    assert!(projection.event.retry.is_none());
    assert_eq!(projection.event.status, LifecycleStatus::Failed);
    assert_eq!(projection.event.severity, axon_api::source::Severity::Fatal);
    assert_eq!(manager.health().await, HealthStatus::Healthy);
    manager.reserve(JobPriority::Interactive, 1).await.unwrap();
}

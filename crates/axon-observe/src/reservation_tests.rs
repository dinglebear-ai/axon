use axon_api::source::{HealthStatus, JobPriority, ProviderId, ProviderKind, ReservationState};
use axon_error::ErrorStage;
use chrono::DateTime;

use crate::reservation::{
    ProviderReservationConfig, ProviderReservationManager, ProviderReservationOutcome,
};

fn manager() -> ProviderReservationManager {
    ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("tei"),
        provider_kind: ProviderKind::Embedding,
        capacity: 2,
        interactive_reserve: 1,
        cooldown_after_failures: 2,
        cooldown_secs: 60,
    })
}

#[tokio::test]
async fn background_reservations_preserve_interactive_capacity() {
    let manager = manager();

    let _background = manager.reserve(JobPriority::Background, 1).await.unwrap();

    let denied = manager
        .reserve(JobPriority::Background, 1)
        .await
        .unwrap_err();
    assert_eq!(denied.code.to_string(), "provider.capacity_exhausted");

    let interactive = manager.reserve(JobPriority::Interactive, 1).await.unwrap();
    assert_eq!(interactive.priority(), JobPriority::Interactive);

    let snapshot = manager.snapshot().await;
    assert_eq!(snapshot.active, 2);
    assert_eq!(snapshot.available_units, 0);
    assert_eq!(snapshot.priority_breakdown.get("background"), Some(&1));
    assert_eq!(snapshot.priority_breakdown.get("interactive"), Some(&1));
    assert!(snapshot.states.contains(&ReservationState::Active));
}

#[tokio::test]
async fn provider_kinds_have_isolated_capacity() {
    let embedding = manager();
    let vector = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("qdrant"),
        provider_kind: ProviderKind::Vector,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });

    let _embedding_hold = embedding
        .reserve(JobPriority::Interactive, 2)
        .await
        .unwrap();
    let vector_hold = vector.reserve(JobPriority::Background, 1).await.unwrap();

    assert_eq!(vector_hold.provider_kind(), ProviderKind::Vector);
    assert_eq!(embedding.snapshot().await.active, 2);
    assert_eq!(vector.snapshot().await.active, 1);
}

#[tokio::test]
async fn reservation_denials_use_leasing_stage_for_non_embedding_providers() {
    let vector = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("qdrant"),
        provider_kind: ProviderKind::Vector,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });

    let _held = vector.reserve(JobPriority::Interactive, 1).await.unwrap();
    let denied = vector
        .reserve(JobPriority::Interactive, 1)
        .await
        .unwrap_err();

    assert_eq!(denied.stage, ErrorStage::Leasing);
    assert_eq!(denied.provider_id, Some("qdrant".to_string()));
}

#[tokio::test]
async fn overridden_reservation_provider_id_is_used_in_denial_errors() {
    let manager = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("embedding-provider-pool"),
        provider_kind: ProviderKind::Embedding,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });

    let _held = manager
        .reserve_for_provider(ProviderId::new("fake-a"), JobPriority::Interactive, 1)
        .await
        .unwrap();
    let denied = manager
        .reserve_for_provider(ProviderId::new("fake-b"), JobPriority::Interactive, 1)
        .await
        .unwrap_err();

    assert_eq!(denied.provider_id, Some("fake-b".to_string()));
}

#[tokio::test]
async fn repeated_retryable_failures_enter_cooldown_and_block_new_reservations() {
    let manager = manager();

    assert_eq!(
        manager.record_failure("provider.timeout", true).await,
        ProviderReservationOutcome::Recorded
    );
    assert_eq!(manager.health().await, HealthStatus::Degraded);

    assert_eq!(
        manager.record_failure("provider.timeout", true).await,
        ProviderReservationOutcome::Cooling
    );
    assert_eq!(manager.health().await, HealthStatus::Cooling);
    assert!(manager.cooldown_until().await.is_some());

    let denied = manager
        .reserve(JobPriority::Interactive, 1)
        .await
        .unwrap_err();
    assert_eq!(denied.code.to_string(), "provider.cooling");
}

#[tokio::test]
async fn successful_probe_clears_cooldown() {
    let manager = manager();
    manager.record_failure("provider.timeout", true).await;
    manager.record_failure("provider.timeout", true).await;
    assert_eq!(manager.health().await, HealthStatus::Cooling);

    manager.record_success().await;

    assert_eq!(manager.health().await, HealthStatus::Healthy);
    assert!(manager.cooldown_until().await.is_none());
    manager.reserve(JobPriority::Interactive, 1).await.unwrap();
}

#[tokio::test]
async fn cooldown_timestamps_are_rfc3339_date_times() {
    let manager = manager();

    manager.record_failure("provider.timeout", true).await;
    manager.record_failure("provider.timeout", true).await;
    let cooldown_until = manager
        .cooldown_until()
        .await
        .expect("cooldown should be set");

    DateTime::parse_from_rfc3339(&cooldown_until.0).expect("cooldown timestamp must be RFC3339");
}

#[tokio::test]
async fn expired_cooldown_allows_new_reservations_without_probe_success() {
    let manager = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("tei"),
        provider_kind: ProviderKind::Embedding,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 0,
    });

    assert_eq!(
        manager.record_failure("provider.timeout", true).await,
        ProviderReservationOutcome::Cooling
    );

    manager.reserve(JobPriority::Interactive, 1).await.unwrap();

    assert_eq!(manager.health().await, HealthStatus::Degraded);
    assert!(manager.cooldown_until().await.is_none());
}

#[tokio::test]
async fn fatal_provider_failure_blocks_reservations_until_success_probe() {
    let manager = manager();

    manager.record_failure("provider.fatal", false).await;
    assert_eq!(manager.health().await, HealthStatus::Unavailable);

    let denied = manager
        .reserve(JobPriority::Interactive, 1)
        .await
        .unwrap_err();
    assert_eq!(denied.code.to_string(), "provider.fatal");
    assert!(!denied.retryable);

    manager.record_success().await;
    manager.reserve(JobPriority::Interactive, 1).await.unwrap();
}

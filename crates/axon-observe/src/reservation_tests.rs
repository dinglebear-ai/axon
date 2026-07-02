use axon_api::source::{
    HealthStatus, JobId, JobPriority, LifecycleStatus, PipelinePhase, ProviderId, ProviderKind,
    ProviderReservationStatus, ReservationState, StageId,
};
use axon_error::ErrorStage;
use chrono::DateTime;

use crate::heartbeat::JobHeartbeatExt;
use crate::reservation::{
    ProviderReservationConfig, ProviderReservationContext, ProviderReservationManager,
    ProviderReservationOutcome,
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

#[tokio::test]
async fn job_aware_reservation_snapshot_carries_observable_context() {
    let manager = manager();
    let job_id = JobId(uuid::Uuid::new_v4());
    let stage_id = StageId(uuid::Uuid::new_v4());

    let reservation = manager
        .reserve_with_context(ProviderReservationContext {
            job_id,
            stage_id: Some(stage_id),
            provider_id: None,
            priority: JobPriority::Background,
            units: 1,
            ttl_seconds: Some(30),
        })
        .await
        .unwrap();
    let snapshot = reservation.snapshot();

    assert!(snapshot.reservation_id.0.starts_with("res_"));
    assert_eq!(snapshot.provider_kind, ProviderKind::Embedding);
    assert_eq!(snapshot.provider_id, Some(ProviderId::new("tei")));
    assert_eq!(snapshot.priority, JobPriority::Background);
    assert_eq!(snapshot.requested_units, 1);
    assert_eq!(snapshot.granted_units, 1);
    assert!(snapshot.acquired_at.is_some());
    assert!(snapshot.expires_at.is_some());
    assert_eq!(snapshot.status, ProviderReservationStatus::Active);
    assert_eq!(reservation.job_id(), Some(job_id));
    assert_eq!(reservation.stage_id(), Some(stage_id));
}

#[tokio::test]
async fn dropping_job_aware_reservation_releases_capacity() {
    let manager = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("tei"),
        provider_kind: ProviderKind::Embedding,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 60,
    });
    let job_id = JobId(uuid::Uuid::new_v4());
    let reservation = manager
        .reserve_with_context(ProviderReservationContext {
            job_id,
            stage_id: None,
            provider_id: None,
            priority: JobPriority::Background,
            units: 1,
            ttl_seconds: None,
        })
        .await
        .unwrap();
    assert_eq!(manager.snapshot().await.active, 1);

    drop(reservation);

    assert_eq!(manager.snapshot().await.active, 0);
}

#[tokio::test]
async fn cooldown_snapshot_is_heartbeat_ready() {
    let manager = manager();
    manager.record_failure("provider.timeout", true).await;
    manager.record_failure("provider.timeout", true).await;

    let snapshot = manager.cooling_snapshot().await.expect("cooling snapshot");
    assert_eq!(snapshot.reason, "provider.timeout");
    assert!(snapshot.retry_after.is_some());
    assert!(snapshot.degraded);

    let heartbeat = crate::heartbeat::heartbeat(
        JobId(uuid::Uuid::new_v4()),
        1,
        PipelinePhase::Embedding,
        LifecycleStatus::Waiting,
    )
    .with_provider_reservations(vec![axon_api::source::ProviderReservationSnapshot {
        reservation_id: axon_api::source::ReservationId::from("res_pending"),
        provider_kind: ProviderKind::Embedding,
        provider_id: Some(ProviderId::new("tei")),
        priority: JobPriority::Background,
        requested_units: 1,
        granted_units: 0,
        acquired_at: None,
        expires_at: None,
        status: ProviderReservationStatus::Queued,
        queue_depth: Some(1),
        cooling: Some(snapshot),
    }]);

    assert_eq!(heartbeat.provider_reservations.len(), 1);
    assert!(heartbeat.provider_reservations[0].cooling.is_some());
}

#[tokio::test]
async fn cooldown_snapshot_keeps_original_started_at() {
    let manager = manager();
    manager.record_failure("provider.timeout", true).await;
    manager.record_failure("provider.timeout", true).await;

    let first = manager.cooling_snapshot().await.expect("cooling snapshot");
    let second = manager.cooling_snapshot().await.expect("cooling snapshot");

    assert_eq!(first.started_at, second.started_at);
}

use axon_api::source::{
    JobId, JobPriority, LifecycleStatus, PipelinePhase, ProviderKind, ReservationId, Severity,
    StageCounts, StageId,
};
use axon_error::{ApiError, ErrorSeverity, ErrorStage};

#[test]
fn stage_event_builders_populate_required_fields() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let stage_id = StageId(uuid::Uuid::new_v4());
    let started = crate::event::stage_started(
        job_id,
        Some(stage_id),
        PipelinePhase::Preparing,
        "preparing".to_string(),
    );

    assert_eq!(started.job_id, job_id);
    assert_eq!(started.stage_id, Some(stage_id));
    assert_eq!(started.status, LifecycleStatus::Running);
    assert_eq!(started.phase, PipelinePhase::Preparing);
    assert_eq!(started.severity, Severity::Info);
    assert_eq!(started.attempt, 1);
    assert!(started.timing.is_some());

    let completed = crate::event::stage_completed(
        job_id,
        Some(stage_id),
        PipelinePhase::Preparing,
        StageCounts {
            items_total: Some(1),
            items_done: 1,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        "prepared".to_string(),
    );
    assert_eq!(completed.status, LifecycleStatus::Completed);
    assert_eq!(completed.counts.items_done, 1);
}

#[test]
fn degraded_and_failed_events_carry_warning_or_error_payloads() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let warning = crate::event::warning("provider.cooling", "provider cooling");
    let degraded = crate::event::stage_degraded(
        job_id,
        None,
        PipelinePhase::Embedding,
        warning.clone(),
        "embedding degraded".to_string(),
    );
    assert_eq!(degraded.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(degraded.warning, Some(warning));
    assert_eq!(degraded.severity, Severity::Degraded);

    let error = ApiError::new(
        "vector.upsert_failed",
        ErrorStage::Upserting,
        "upsert failed",
    )
    .with_severity(ErrorSeverity::Failed);
    let failed = crate::event::stage_failed(
        job_id,
        None,
        PipelinePhase::Upserting,
        error.clone(),
        "upsert failed".to_string(),
    );
    assert_eq!(failed.status, LifecycleStatus::Failed);
    assert_eq!(failed.error, Some(error));
    assert_eq!(failed.severity, Severity::Failed);
}

#[test]
fn provider_wait_event_exposes_reservation_context() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let reservation_id = ReservationId::from("res_1");
    let event = crate::event::provider_waiting(
        job_id,
        None,
        Some(reservation_id.clone()),
        ProviderKind::Embedding,
        JobPriority::Background,
        "waiting for embedding provider".to_string(),
    );

    assert_eq!(event.status, LifecycleStatus::Waiting);
    assert_eq!(event.reservation_id, Some(reservation_id));
    assert_eq!(event.current.unwrap().provider.unwrap().0, "embedding");
    assert_eq!(event.dedupe_key.as_deref(), Some("provider_wait:embedding"));
}

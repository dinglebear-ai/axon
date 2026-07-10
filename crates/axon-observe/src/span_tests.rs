use super::*;
use axon_api::source::{
    AdapterRef, ApiError, JobId, LifecycleStatus, ProgressCurrent, SourceId, Timestamp, Visibility,
};
use axon_error::{ErrorSeverity, ErrorStage, ErrorVisibility};

fn base_event(job_id: JobId) -> SourceProgressEvent {
    let now = Timestamp::from(chrono::Utc::now());
    SourceProgressEvent {
        event_id: "evt_1".to_string(),
        sequence: 0,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Embedding,
        status: LifecycleStatus::Running,
        severity: Severity::Info,
        visibility: Visibility::Internal,
        message: "embedding chunk".to_string(),
        timestamp: now,
        source_id: Some(SourceId::from("src_1")),
        canonical_uri: None,
        adapter: Some(AdapterRef {
            name: "github".to_string(),
            version: "1.0.0".to_string(),
        }),
        scope: Some(SourceScope::Repo),
        generation: None,
        counts: crate::event::zero_counts(),
        timing: None,
        current: Some(ProgressCurrent {
            source_item_key: None,
            document_id: None,
            chunk_id: None,
            adapter: None,
            provider: Some(ProviderId::from("tei")),
            message: None,
        }),
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    }
}

#[test]
fn from_event_carries_bounded_identifier_fields() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let event = base_event(job_id);

    let fields = SpanFieldSet::from_event(&event);

    assert_eq!(fields.job_id, Some(job_id));
    assert_eq!(fields.source_id, Some(SourceId::from("src_1")));
    assert_eq!(fields.adapter.as_deref(), Some("github"));
    assert_eq!(fields.scope, Some(SourceScope::Repo));
    assert_eq!(fields.phase, Some(PipelinePhase::Embedding));
    assert_eq!(fields.provider_id, Some(ProviderId::from("tei")));
    assert_eq!(fields.severity, Some(Severity::Info));
    assert!(fields.error_code.is_none());
}

#[test]
fn from_event_extracts_error_code_when_present() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let mut event = base_event(job_id);
    event.error = Some(ApiError {
        code: "provider.unavailable".into(),
        message: "provider down".to_string(),
        stage: ErrorStage::Embedding,
        retryable: true,
        severity: ErrorSeverity::Failed,
        visibility: ErrorVisibility::Public,
        details: Default::default(),
        job_id: None,
        source_id: None,
        source_item_key: None,
        document_id: None,
        chunk_id: None,
        provider_id: None,
        retry_after_ms: None,
        cooldown_until: None,
    });

    let fields = SpanFieldSet::from_event(&event);
    assert_eq!(fields.error_code.as_deref(), Some("provider.unavailable"));
}

#[test]
fn from_heartbeat_carries_job_and_phase() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let heartbeat =
        crate::heartbeat::heartbeat(job_id, 1, PipelinePhase::Fetching, LifecycleStatus::Running);

    let fields = SpanFieldSet::from_heartbeat(&heartbeat);
    assert_eq!(fields.job_id, Some(job_id));
    assert_eq!(fields.phase, Some(PipelinePhase::Fetching));
    assert!(fields.source_id.is_none());
}

#[test]
fn field_set_round_trips_through_json() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let fields = SpanFieldSet::from_event(&base_event(job_id));

    let json = serde_json::to_value(&fields).expect("serialize span fields");
    let round_trip: SpanFieldSet = serde_json::from_value(json).expect("deserialize span fields");
    assert_eq!(round_trip, fields);
}

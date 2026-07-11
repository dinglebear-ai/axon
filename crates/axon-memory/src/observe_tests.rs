use super::*;
use axon_api::source::{MemoryHistoryEvent, MemoryScope, MemoryStatus, MemoryType, Timestamp};
use axon_observe::testing::InMemoryObservabilitySink;
use std::sync::Arc;

fn sample_record() -> MemoryRecord {
    MemoryRecord {
        memory_id: axon_api::source::MemoryId::new("mem_test".to_string()),
        memory_type: MemoryType::Fact,
        status: MemoryStatus::Active,
        body: "body".to_string(),
        confidence: 0.5,
        salience: 0.5,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status: MemoryStatus::Active,
            message: "created".to_string(),
            timestamp: Timestamp("2026-01-01T00:00:00Z".to_string()),
        }],
        visibility: Visibility::Internal,
        title: None,
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

#[tokio::test]
async fn emits_remembering_event_with_required_fields() {
    let concrete = InMemoryObservabilitySink::default();
    let sink: Arc<dyn ObservabilitySink> = Arc::new(concrete.clone());
    let record = sample_record();
    emit(
        &sink,
        MemoryPhase::Remembering,
        &record,
        Severity::Info,
        None,
        Some(0.8),
        None,
    )
    .await;
    let snapshot = concrete.snapshot();
    assert_eq!(snapshot.events.len(), 1);
    let event = &snapshot.events[0];
    assert_eq!(event.phase, PipelinePhase::Preparing);
    assert_eq!(event.canonical_uri.as_deref(), Some("memory://mem_test"));
    assert!(event.message.contains("phase=remembering"));
    assert!(event.message.contains("memory_id=mem_test"));
    assert!(event.message.contains("memory_type=fact"));
    assert!(event.message.contains("memory_status=active"));
    assert!(event.message.contains("memory_scope_kind=project"));
    assert!(event.message.contains("score_after=0.8"));
}

#[tokio::test]
async fn emits_review_event_with_reason() {
    let concrete = InMemoryObservabilitySink::default();
    let sink: Arc<dyn ObservabilitySink> = Arc::new(concrete.clone());
    let mut record = sample_record();
    record.status = MemoryStatus::Contradicted;

    emit(
        &sink,
        MemoryPhase::Reviewing,
        &record,
        Severity::Warning,
        Some(0.6),
        Some(0.35),
        Some("contradiction"),
    )
    .await;

    let snapshot = concrete.snapshot();
    assert_eq!(snapshot.events.len(), 1);
    let event = &snapshot.events[0];
    assert_eq!(event.phase, PipelinePhase::Evaluating);
    assert_eq!(event.severity, Severity::Warning);
    assert!(event.message.contains("review_reason=contradiction"));
    assert!(event.message.contains("score_before=0.6"));
    assert!(event.message.contains("score_after=0.35"));
}

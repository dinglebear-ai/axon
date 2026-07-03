use super::*;
use axon_api::source::{JobId, LifecycleStatus, MetadataMap, PipelinePhase, Timestamp};
use chrono::Utc;

use crate::collector::ObservabilitySink;
use crate::heartbeat::heartbeat;
use crate::metric::MetricSample;

#[tokio::test]
async fn tracing_sink_assigns_monotonic_sequence() {
    let sink = TracingObservabilitySink::new();
    let job = JobId(uuid::Uuid::new_v4());

    // Emit twice; the shared registry advances the per-job sequence.
    sink.emit(crate::event::stage_started(
        job,
        None,
        PipelinePhase::Fetching,
        "one".to_string(),
    ))
    .await
    .unwrap();
    sink.emit(crate::event::stage_started(
        job,
        None,
        PipelinePhase::Fetching,
        "two".to_string(),
    ))
    .await
    .unwrap();

    assert_eq!(sink.sequences().last(job), Some(2));
}

#[tokio::test]
async fn heartbeat_and_metric_and_flush_succeed() {
    let sink = TracingObservabilitySink::new();
    let job = JobId(uuid::Uuid::new_v4());

    sink.heartbeat(heartbeat(
        job,
        1,
        PipelinePhase::Embedding,
        LifecycleStatus::Running,
    ))
    .await
    .unwrap();
    sink.metric(MetricSample {
        name: "axon_jobs_active".to_string(),
        value: 3.0,
        unit: None,
        labels: MetadataMap::default(),
        timestamp: Timestamp::from(Utc::now()),
    })
    .await
    .unwrap();
    sink.flush().await.unwrap();
}

#[tokio::test]
async fn shared_registry_keeps_sinks_in_lockstep() {
    let seq = std::sync::Arc::new(crate::sequence::SequenceRegistry::new());
    let a = TracingObservabilitySink::with_sequences(std::sync::Arc::clone(&seq));
    let b = TracingObservabilitySink::with_sequences(std::sync::Arc::clone(&seq));
    let job = JobId(uuid::Uuid::new_v4());

    a.emit(crate::event::stage_started(
        job,
        None,
        PipelinePhase::Fetching,
        "a".into(),
    ))
    .await
    .unwrap();
    b.emit(crate::event::stage_started(
        job,
        None,
        PipelinePhase::Fetching,
        "b".into(),
    ))
    .await
    .unwrap();

    // Two distinct sinks sharing one registry produced sequences 1 then 2.
    assert_eq!(seq.last(job), Some(2));
}

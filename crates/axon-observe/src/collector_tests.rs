use axon_api::source::{
    JobId, JobPriority, LifecycleStatus, MetadataMap, PipelinePhase, ProviderKind, Timestamp,
};
use chrono::Utc;

use crate::collector::{NoopObservabilitySink, ObservabilitySink};
use crate::metric::MetricSample;
use crate::testing::InMemoryObservabilitySink;

#[tokio::test]
async fn in_memory_sink_records_emit_heartbeat_metric_flush_in_order() {
    let sink = InMemoryObservabilitySink::default();
    let job_id = JobId(uuid::Uuid::new_v4());
    let event = crate::event::stage_started(
        job_id,
        None,
        PipelinePhase::Fetching,
        "fetching".to_string(),
    );
    let heartbeat =
        crate::heartbeat::heartbeat(job_id, 1, PipelinePhase::Fetching, LifecycleStatus::Running);
    let metric = MetricSample {
        name: "axon_job_events_total".to_string(),
        value: 1.0,
        unit: Some("count".to_string()),
        labels: MetadataMap::default(),
        timestamp: Timestamp::from(Utc::now()),
    };

    sink.emit(event.clone()).await.unwrap();
    sink.heartbeat(heartbeat.clone()).await.unwrap();
    sink.metric(metric.clone()).await.unwrap();
    sink.flush().await.unwrap();

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.events, vec![event]);
    assert_eq!(snapshot.heartbeats, vec![heartbeat]);
    assert_eq!(snapshot.metrics, vec![metric]);
    assert_eq!(
        snapshot.operations,
        vec![
            "emit".to_string(),
            "heartbeat".to_string(),
            "metric".to_string(),
            "flush".to_string()
        ]
    );
}

#[tokio::test]
async fn noop_sink_accepts_all_calls() {
    let sink = NoopObservabilitySink;
    let job_id = JobId(uuid::Uuid::new_v4());

    sink.emit(crate::event::provider_waiting(
        job_id,
        None,
        None,
        ProviderKind::Embedding,
        JobPriority::Background,
        "waiting".to_string(),
    ))
    .await
    .unwrap();
    sink.heartbeat(crate::heartbeat::heartbeat(
        job_id,
        1,
        PipelinePhase::Embedding,
        LifecycleStatus::Waiting,
    ))
    .await
    .unwrap();
    sink.metric(MetricSample {
        name: "axon_provider_waits_total".to_string(),
        value: 1.0,
        unit: None,
        labels: MetadataMap::default(),
        timestamp: Timestamp::from(Utc::now()),
    })
    .await
    .unwrap();
    sink.flush().await.unwrap();
}

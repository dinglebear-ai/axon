use super::*;
use axon_api::source::{
    JobId, LifecycleStatus, PipelinePhase, ProviderId, ProviderKind, Timestamp,
};
use chrono::Utc;

use crate::collector::ObservabilitySink;
use crate::heartbeat::heartbeat;
use crate::metric::MetricSample;

async fn sink() -> SqliteObservabilitySink {
    SqliteObservabilitySink::connect(":memory:")
        .await
        .expect("connect in-memory sink")
}

fn event(job_id: JobId, phase: PipelinePhase) -> SourceProgressEvent {
    crate::event::stage_started(job_id, None, phase, "stage".to_string())
}

#[tokio::test]
async fn emit_persists_event_and_reads_back() {
    let sink = sink().await;
    let job = JobId(uuid::Uuid::new_v4());

    sink.emit(event(job, PipelinePhase::Fetching))
        .await
        .unwrap();

    let stored = sink.events_for(job).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].job_id, job);
    assert_eq!(stored[0].phase, PipelinePhase::Fetching);
    // The builder placeholder (0) must have been replaced by the assigned seq.
    assert_eq!(stored[0].sequence, 1);
}

#[tokio::test]
async fn sequence_strictly_increases_across_n_emits() {
    let sink = sink().await;
    let job = JobId(uuid::Uuid::new_v4());

    const N: u64 = 25;
    for _ in 0..N {
        sink.emit(event(job, PipelinePhase::Embedding))
            .await
            .unwrap();
    }

    let stored = sink.events_for(job).await.unwrap();
    assert_eq!(stored.len() as u64, N);
    for (idx, ev) in stored.iter().enumerate() {
        assert_eq!(ev.sequence, idx as u64 + 1);
    }
    for window in stored.windows(2) {
        assert!(
            window[1].sequence > window[0].sequence,
            "sequence must strictly increase within a stream"
        );
    }
}

#[tokio::test]
async fn sequences_are_independent_per_job() {
    let sink = sink().await;
    let a = JobId(uuid::Uuid::new_v4());
    let b = JobId(uuid::Uuid::new_v4());

    sink.emit(event(a, PipelinePhase::Fetching)).await.unwrap();
    sink.emit(event(b, PipelinePhase::Fetching)).await.unwrap();
    sink.emit(event(a, PipelinePhase::Fetching)).await.unwrap();

    let a_events = sink.events_for(a).await.unwrap();
    let b_events = sink.events_for(b).await.unwrap();
    assert_eq!(
        a_events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        b_events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1]
    );
}

#[tokio::test]
async fn heartbeat_upserts_and_stamps_last_sequence() {
    let sink = sink().await;
    let job = JobId(uuid::Uuid::new_v4());

    sink.emit(event(job, PipelinePhase::Fetching))
        .await
        .unwrap();
    sink.emit(event(job, PipelinePhase::Fetching))
        .await
        .unwrap();

    let hb = heartbeat(job, 1, PipelinePhase::Fetching, LifecycleStatus::Running);
    sink.heartbeat(hb).await.unwrap();

    let stored = sink
        .heartbeat_for(job)
        .await
        .unwrap()
        .expect("heartbeat row");
    assert_eq!(stored.job_id, job);
    assert_eq!(stored.status, LifecycleStatus::Running);
    // last durable sequence observed on this stream is 2.
    assert_eq!(stored.last_event_sequence, Some(2));

    // Second heartbeat upserts (single row per job).
    let hb2 = heartbeat(job, 2, PipelinePhase::Embedding, LifecycleStatus::Running);
    sink.heartbeat(hb2).await.unwrap();
    let stored2 = sink.heartbeat_for(job).await.unwrap().unwrap();
    assert_eq!(stored2.attempt, 2);
    assert_eq!(stored2.phase, PipelinePhase::Embedding);
}

#[tokio::test]
async fn flush_succeeds_after_terminal_event() {
    let sink = sink().await;
    let job = JobId(uuid::Uuid::new_v4());
    sink.emit(crate::event::stage_completed(
        job,
        None,
        PipelinePhase::Complete,
        axon_api::source::StageCounts {
            items_total: Some(3),
            items_done: 3,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        "done".to_string(),
    ))
    .await
    .unwrap();
    sink.flush().await.unwrap();
}

#[tokio::test]
async fn metric_rejects_unbounded_labels() {
    let sink = sink().await;
    let mut labels = axon_api::source::MetadataMap::new();
    labels.insert("url".to_string(), serde_json::json!("https://x"));
    let bad = MetricSample {
        name: "axon_embeddings_total".to_string(),
        value: 1.0,
        unit: None,
        labels,
        timestamp: Timestamp::from(Utc::now()),
    };
    let err = sink.metric(bad).await.unwrap_err();
    assert_eq!(err.code.0, "observe.unbounded_label");

    // Bounded labels are accepted.
    let mut ok_labels = axon_api::source::MetadataMap::new();
    ok_labels.insert("provider".to_string(), serde_json::json!("tei"));
    let good = MetricSample {
        name: "axon_embeddings_total".to_string(),
        value: 1.0,
        unit: None,
        labels: ok_labels,
        timestamp: Timestamp::from(Utc::now()),
    };
    sink.metric(good).await.unwrap();
}

#[tokio::test]
async fn provider_health_upserts() {
    let sink = sink().await;
    let provider = ProviderId::from("tei-qwen3");

    sink.record_provider_health(ProviderHealthRecord {
        provider_id: provider.clone(),
        provider_kind: ProviderKind::Embedding,
        status: "cooling".to_string(),
        cooldown_until: Some(Timestamp::from(Utc::now())),
        last_error_code: Some("tei.rate_limited".to_string()),
    })
    .await
    .unwrap();

    let stored = sink
        .provider_health_for(&provider)
        .await
        .unwrap()
        .expect("provider health row");
    assert_eq!(stored.provider_kind, ProviderKind::Embedding);
    assert_eq!(stored.status, "cooling");
    assert_eq!(stored.last_error_code.as_deref(), Some("tei.rate_limited"));

    // Upsert to ready.
    sink.record_provider_health(ProviderHealthRecord {
        provider_id: provider.clone(),
        provider_kind: ProviderKind::Embedding,
        status: "ready".to_string(),
        cooldown_until: None,
        last_error_code: None,
    })
    .await
    .unwrap();
    let ready = sink.provider_health_for(&provider).await.unwrap().unwrap();
    assert_eq!(ready.status, "ready");
    assert_eq!(ready.cooldown_until, None);
}

#[tokio::test]
async fn duplicate_sequence_insert_is_rejected_by_unique_index() {
    // Persisting the same (job_id, sequence) twice violates the durable guard.
    let sink = sink().await;
    let job = JobId(uuid::Uuid::new_v4());
    let mut ev = event(job, PipelinePhase::Fetching);
    ev.sequence = 0; // will be reassigned to 1 by the sink

    // Directly hit the pool to force a duplicate sequence row.
    let mut a = ev.clone();
    a.event_id = "evt_a".to_string();
    a.sequence = 5;
    let mut b = ev.clone();
    b.event_id = "evt_b".to_string();
    b.sequence = 5;

    insert_raw(&sink, &a).await.unwrap();
    let dup = insert_raw(&sink, &b).await;
    assert!(dup.is_err(), "duplicate (job_id, sequence) must fail");
}

async fn insert_raw(
    sink: &SqliteObservabilitySink,
    ev: &SourceProgressEvent,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO axon_observe_events \
         (event_id, job_id, sequence, phase, status, severity, visibility, message, \
          timestamp, event_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&ev.event_id)
    .bind(ev.job_id.0.to_string())
    .bind(i64::try_from(ev.sequence).unwrap())
    .bind(enum_str(&ev.phase))
    .bind(enum_str(&ev.status))
    .bind(enum_str(&ev.severity))
    .bind(enum_str(&ev.visibility))
    .bind(&ev.message)
    .bind(&ev.timestamp.0)
    .bind(serde_json::to_string(ev).unwrap())
    .bind(now_ms())
    .execute(&sink.pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn on_disk_sink_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("observe.db");
    let path_str = path.to_str().unwrap();
    let job = JobId(uuid::Uuid::new_v4());

    {
        let sink = SqliteObservabilitySink::connect(path_str).await.unwrap();
        sink.emit(event(job, PipelinePhase::Fetching))
            .await
            .unwrap();
        sink.flush().await.unwrap();
    }

    // Reopen: rows survive; a fresh registry restarts sequence numbering, but
    // the durable unique index still guards the persisted stream.
    let reopened = SqliteObservabilitySink::connect(path_str).await.unwrap();
    let stored = reopened.events_for(job).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].sequence, 1);
}

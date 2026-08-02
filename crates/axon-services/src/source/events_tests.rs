use std::sync::Arc;

use axon_api::source::*;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

use super::{SourceEventDetails, SourceEventEmitter};

#[tokio::test]
async fn emitter_persists_structured_counts_and_source_context() {
    let (store, job_id) = store_with_job().await;
    let emitter = emitter(store.clone(), job_id);
    let counts = StageCounts {
        items_total: Some(3),
        items_done: 2,
        documents_total: Some(2),
        documents_done: 2,
        chunks_total: Some(8),
        chunks_done: 8,
        bytes_total: Some(128),
        bytes_done: 128,
    };

    emitter
        .completed_with(
            PipelinePhase::Preparing,
            "prepared documents",
            SourceEventDetails {
                generation: Some(SourceGenerationId::new("gen_1")),
                counts: Some(counts.clone()),
                ..SourceEventDetails::default()
            },
        )
        .await;

    let event = recorded_progress(&store, job_id, 0).await;
    assert_eq!(event.source_id, Some(SourceId::new("src_test")));
    assert_eq!(event.canonical_uri.as_deref(), Some("https://example.com"));
    assert_eq!(event.scope, Some(SourceScope::Site));
    assert_eq!(event.generation, Some(SourceGenerationId::new("gen_1")));
    assert_eq!(event.counts, counts);
}

#[tokio::test]
async fn running_event_carries_the_authoritative_progress_counts() {
    let (store, job_id) = store_with_job().await;
    let emitter = emitter(store.clone(), job_id);
    let counts = StageCounts {
        items_total: Some(10),
        items_done: 4,
        documents_total: Some(10),
        documents_done: 4,
        chunks_total: Some(100),
        chunks_done: 40,
        bytes_total: None,
        bytes_done: 0,
    };

    emitter
        .running_with_counts(
            PipelinePhase::Embedding,
            "embedding chunks",
            Some(counts.clone()),
        )
        .await;

    let event = recorded_progress(&store, job_id, 0).await;
    assert_eq!(event.status, LifecycleStatus::Running);
    assert_eq!(event.phase, PipelinePhase::Embedding);
    assert_eq!(event.counts, counts);
}

#[tokio::test]
async fn emitter_persists_item_warning_and_error_payloads() {
    let (store, job_id) = store_with_job().await;
    let emitter = emitter(store.clone(), job_id);
    let item = SourceItemKey::new("page:/docs");
    emitter
        .warning(
            PipelinePhase::Fetching,
            SourceWarning {
                code: "web.render.degraded".to_string(),
                severity: Severity::Degraded,
                message: "used fallback renderer".to_string(),
                source_item_key: Some(item.clone()),
                retryable: true,
            },
            Some(SourceGenerationId::new("gen_1")),
        )
        .await;
    emitter
        .item_error(
            PipelinePhase::Diffing,
            SourceError {
                code: "source.item.invalid".to_string(),
                severity: Severity::Degraded,
                message: "item could not be diffed".to_string(),
                source_item_key: Some(item.clone()),
                retryable: false,
                provider_id: Some(ProviderId::new("provider_test")),
                cause: None,
            },
            Some(SourceGenerationId::new("gen_1")),
        )
        .await;

    let warning = recorded_progress(&store, job_id, 0).await;
    assert_eq!(
        warning.warning.as_ref().map(|value| value.code.as_str()),
        Some("web.render.degraded")
    );
    assert_eq!(
        warning.current.and_then(|value| value.source_item_key),
        Some(item.clone())
    );

    let error = recorded_progress(&store, job_id, 1).await;
    assert_eq!(
        error.error.as_ref().map(|value| value.code.0.as_str()),
        Some("source.item.invalid")
    );
    assert_eq!(
        error.current.and_then(|value| value.source_item_key),
        Some(item)
    );
}

#[tokio::test]
async fn concurrent_emitters_use_store_allocated_sequences() {
    let (store, job_id) = store_with_job().await;
    let first = emitter(store.clone(), job_id);
    let second = emitter(store.clone(), job_id);

    tokio::join!(
        first.completed(PipelinePhase::Fetching, "first"),
        second.completed(PipelinePhase::Preparing, "second"),
    );

    let events = store.recorded_events(job_id).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
}

fn emitter(store: Arc<FakeJobWatchStore>, job_id: JobId) -> SourceEventEmitter {
    SourceEventEmitter::new(Some(store), Some(job_id))
        .with_route(
            SourceKind::Web,
            SourceScope::Site,
            AdapterRef {
                name: "web".to_string(),
                version: "test".to_string(),
            },
        )
        .with_source(SourceId::new("src_test"), "https://example.com")
}

async fn store_with_job() -> (Arc<FakeJobWatchStore>, JobId) {
    let store = Arc::new(FakeJobWatchStore::new());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Run,
            source_id: Some(SourceId::new("src_test")),
            watch_id: None,
            parent_job_id: None,
            root_job_id: None,
            attempt: 1,
            priority: JobPriority::Normal,
            idempotency_key: None,
            stage_plan: Vec::new(),
            request: None,
            auth_snapshot: AuthSnapshot::trusted_system("test"),
            config_snapshot_id: Some(ConfigSnapshotId::new("cfg_test")),
            requirements: MetadataMap::new(),
            result_schema: Some("source_result".to_string()),
            warnings: Vec::new(),
            error: None,
            metadata: MetadataMap::new(),
            deadline_at: None,
        })
        .await
        .expect("create job");
    (store, descriptor.job_id)
}

async fn recorded_progress(
    store: &FakeJobWatchStore,
    job_id: JobId,
    index: usize,
) -> SourceProgressEvent {
    let events = store.recorded_events(job_id).await;
    serde_json::from_value(
        events[index]
            .details
            .get("source_progress_event")
            .cloned()
            .expect("progress payload"),
    )
    .expect("deserialize progress event")
}

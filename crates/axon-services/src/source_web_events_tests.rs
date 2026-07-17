//! Structured `SourceProgressEvent` projection tests for the web source
//! pipeline: phase completions carry source identity, generation, and stage
//! counts; terminal failures carry a structured error; and each run emits
//! exactly one terminal-phase event.
//!
//! See the module doc on `web_source_tests.rs` for why these use
//! `SourceScope::Page` + `FakeAdapterProviders`.

use std::sync::Arc;

use axon_adapters::boundary::FakeAdapterProviders;
use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::{FakeVectorMode, FakeVectorStore};

use super::{WebSourceIndexInput, index_web_source};

fn input(store: Arc<FakeJobWatchStore>, job_id: JobId) -> WebSourceIndexInput {
    let providers = Arc::new(FakeAdapterProviders::new());
    WebSourceIndexInput {
        source: "https://example.com/docs?utm_source=noise".to_string(),
        scope: SourceScope::Page,
        map_urls: Vec::new(),
        crawl_options: MetadataMap::new(),
        output: OutputPolicy::default(),
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id,
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        attempt: 1,
        embed: true,
        fetch_provider: providers.clone(),
        render_provider: providers,
        artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        document_cache: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        event_store: Some(store),
    }
}

#[tokio::test]
async fn web_refresh_emits_structured_phase_completions() {
    let (store, job_id) = store_with_job().await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_web_source(input(store.clone(), job_id), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    let events = progress_events(&store, job_id).await;
    let discovered = completed_event(&events, PipelinePhase::Discovering);
    assert_eq!(discovered.source_id, Some(output.source_id.clone()));
    assert!(
        discovered
            .canonical_uri
            .as_deref()
            .is_some_and(|uri| uri.starts_with("https://example.com/docs")),
        "unexpected canonical uri: {:?}",
        discovered.canonical_uri
    );
    assert_eq!(discovered.scope, Some(SourceScope::Page));
    assert_eq!(discovered.counts.items_total, Some(1));

    completed_event(&events, PipelinePhase::Diffing);

    let published = completed_event(&events, PipelinePhase::Publishing);
    assert_eq!(published.generation, Some(output.generation.clone()));
    assert_eq!(
        published.counts.documents_done, output.documents_prepared,
        "published counts must mirror the output"
    );
    assert_eq!(published.counts.chunks_done, output.chunks_prepared);

    let last = events.last().expect("progress events recorded");
    assert_eq!(last.phase, PipelinePhase::Publishing);
    assert_eq!(last.status, LifecycleStatus::Completed);
    assert!(
        events
            .iter()
            .all(|event| event.status != LifecycleStatus::Failed),
        "successful run must not record failed events"
    );
}

#[tokio::test]
async fn web_pipeline_failure_emits_one_structured_terminal_failure() {
    let (store, job_id) = store_with_job().await;
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector").with_mode(FakeVectorMode::PartialFailure);

    index_web_source(input(store.clone(), job_id), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    let events = progress_events(&store, job_id).await;
    let failed = events
        .iter()
        .filter(|event| event.status == LifecycleStatus::Failed)
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1, "exactly one terminal failure event");
    let failure = failed[0];
    assert_eq!(failure.phase, PipelinePhase::Complete);
    assert_eq!(
        failure.error.as_ref().map(|error| error.code.0.as_str()),
        Some("source.index_failed")
    );
    assert!(failure.source_id.is_some());
    assert_eq!(
        events.last().map(|event| event.status),
        Some(LifecycleStatus::Failed),
        "terminal failure must be the final event"
    );
}

fn completed_event(events: &[SourceProgressEvent], phase: PipelinePhase) -> &SourceProgressEvent {
    events
        .iter()
        .find(|event| event.phase == phase && event.status == LifecycleStatus::Completed)
        .unwrap_or_else(|| panic!("missing completed event for {phase:?}"))
}

async fn progress_events(store: &FakeJobWatchStore, job_id: JobId) -> Vec<SourceProgressEvent> {
    store
        .recorded_events(job_id)
        .await
        .into_iter()
        .map(|event| {
            serde_json::from_value(
                event
                    .details
                    .get("source_progress_event")
                    .cloned()
                    .expect("progress payload"),
            )
            .expect("deserialize progress event")
        })
        .collect()
}

async fn store_with_job() -> (Arc<FakeJobWatchStore>, JobId) {
    let store = Arc::new(FakeJobWatchStore::new());
    let descriptor = store
        .create(JobCreateRequest {
            request_id: None,
            job_kind: JobKind::Source,
            job_intent: JobIntent::Run,
            source_id: None,
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

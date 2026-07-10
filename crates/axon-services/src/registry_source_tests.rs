use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use crate::test_support::committed_generation_payload;

use super::{RegistrySourceIndexInput, index_registry_source, index_registry_source_with_job};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
}

fn write_dump(json: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "axon-registry-source-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create registry source test dir");
    let path = dir.join("dump.json");
    std::fs::write(&path, json).expect("failed to write registry dump fixture");
    path
}

fn valid_dump_json() -> &'static str {
    r##"{
        "registry": "npm",
        "package": "lodash",
        "description": "Lodash modular utilities.",
        "homepage": "https://lodash.com",
        "license": "MIT",
        "author": "jdd",
        "keywords": ["array", "util"],
        "versions": [
            {
                "version": "4.17.20",
                "readme": "# lodash 4.17.20\n\nOlder release.",
                "is_latest": false
            },
            {
                "version": "4.17.21",
                "readme": "# lodash\n\nA modern JavaScript utility library.",
                "description": "Lodash modular utilities.",
                "published_at": "2021-02-20T00:00:00Z",
                "is_latest": true
            }
        ]
    }"##
}

fn input(dump_path: std::path::PathBuf) -> RegistrySourceIndexInput {
    RegistrySourceIndexInput {
        registry_dump_path: dump_path,
        include_all_versions: false,
        collection: "axon-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        embedding_reservations: None,
        vector_reservations: None,
        embed: true,
        max_items: None,
    }
}

fn input_with_reservations(dump_path: std::path::PathBuf) -> RegistrySourceIndexInput {
    let mut input = input(dump_path);
    input.embedding_reservations = Some(Arc::new(ProviderReservationManager::new(
        ProviderReservationConfig {
            provider_id: input.embedding_provider_id.clone(),
            provider_kind: ProviderKind::Embedding,
            capacity: 2,
            interactive_reserve: 1,
            cooldown_after_failures: 1,
            cooldown_secs: 30,
        },
    )));
    input.vector_reservations = Some(Arc::new(ProviderReservationManager::new(
        ProviderReservationConfig {
            provider_id: input.vector_provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            capacity: 2,
            interactive_reserve: 1,
            cooldown_after_failures: 1,
            cooldown_secs: 30,
        },
    )));
    input
}

#[tokio::test]
async fn registry_source_refresh_writes_vectors_then_commits_source_generation() {
    let dump_path = write_dump(valid_dump_json());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_registry_source(input(dump_path.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(super::registry_source_id(&dump_path), output.source_id);
    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert!(output.documents_prepared >= 1);
    assert!(output.chunks_prepared >= 1);
    assert!(output.vector_points_written >= 1);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["committed_generation"]
                == committed_generation_payload(&output.generation))
    );
    assert!(
        vectors
            .points("axon-test")
            .await
            .iter()
            .all(|point| point.payload["document_status"].as_str() == Some("published"))
    );
    for point in vectors.points("axon-test").await {
        let status = ledger
            .document_status(&DocumentId::new(
                point.payload["document_id"].as_str().unwrap(),
            ))
            .await
            .expect("ledger document status");
        assert_eq!(status.status, DocumentLifecycleStatus::Published);
    }
    let source = ledger
        .get_source(output.source_id.clone())
        .await
        .unwrap()
        .expect("source summary");
    assert_eq!(source.status, LifecycleStatus::Completed);
    // Default (include_all_versions = false) discovers only the latest version.
    assert_eq!(source.counts.items_total, 1);
    assert_eq!(source.counts.documents_total, output.documents_prepared);
}

#[tokio::test]
async fn registry_source_include_all_versions_discovers_every_version() {
    let dump_path = write_dump(valid_dump_json());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let mut request = input(dump_path);
    request.include_all_versions = true;

    let output = index_registry_source(request, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    let source = ledger
        .get_source(output.source_id.clone())
        .await
        .unwrap()
        .expect("source summary");
    assert_eq!(source.counts.items_total, 2);
    assert_eq!(output.documents_prepared, 2);
}

#[tokio::test]
async fn registry_source_job_emits_progress_events_for_pipeline_phases() {
    let dump_path = write_dump(valid_dump_json());
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output =
        index_registry_source_with_job(input(dump_path), &jobs, &ledger, &embedder, &vectors)
            .await
            .unwrap();

    let summary = JobStore::get(&jobs, output.job_id)
        .await
        .unwrap()
        .expect("job summary");
    assert_eq!(summary.kind, JobKind::Source);
    assert_eq!(summary.status, LifecycleStatus::Completed);
    assert_eq!(summary.source_id, Some(output.source_id.clone()));

    let events = JobStore::events(
        &jobs,
        JobEventListRequest {
            job_id: output.job_id,
            after_sequence: None,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(20),
            cursor: None,
        },
    )
    .await
    .unwrap();
    let phases = events
        .events
        .iter()
        .map(|event| event.phase)
        .collect::<Vec<_>>();

    assert_eq!(
        phases,
        vec![
            PipelinePhase::Discovering,
            PipelinePhase::Diffing,
            PipelinePhase::Preparing,
            PipelinePhase::Embedding,
            PipelinePhase::Vectorizing,
            PipelinePhase::Publishing,
            PipelinePhase::Cleaning,
            PipelinePhase::Complete,
        ]
    );
    assert!(
        events
            .events
            .iter()
            .all(|event| event.job_id == output.job_id)
    );
}

#[tokio::test]
async fn registry_source_job_records_provider_reservation_events() {
    let dump_path = write_dump(valid_dump_json());
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_registry_source_with_job(
        input_with_reservations(dump_path),
        &jobs,
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    let events = JobStore::events(
        &jobs,
        JobEventListRequest {
            job_id: output.job_id,
            after_sequence: None,
            phase: None,
            severity: None,
            visibility: Some(Visibility::Public),
            since_sequence: None,
            limit: Some(20),
            cursor: None,
        },
    )
    .await
    .unwrap();
    let embedding_event = events
        .events
        .iter()
        .find(|event| event.phase == PipelinePhase::Embedding)
        .expect("embedding event");
    assert!(
        progress_reservation_id(embedding_event).is_some(),
        "embedding phase should expose reservation evidence"
    );
    let vectorizing_event = events
        .events
        .iter()
        .find(|event| event.phase == PipelinePhase::Vectorizing)
        .expect("vectorizing event");
    assert!(
        progress_reservation_id(vectorizing_event).is_some(),
        "vectorizing phase should expose reservation evidence"
    );
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors"): package versions are still discovered/prepared
/// (documents_prepared stays non-zero) but neither the embedding provider
/// nor `vector_store.upsert` may be called.
#[tokio::test]
async fn embed_false_prepares_versions_but_writes_no_vectors() {
    let dump_path = write_dump(valid_dump_json());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(dump_path);
    no_embed_input.include_all_versions = true;
    no_embed_input.embed = false;

    let output = index_registry_source(no_embed_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(
        embedder.calls().await.len(),
        0,
        "embed=false must not call the embedding provider"
    );
    assert!(
        !vectors.calls().await.contains(&"upsert"),
        "embed=false must not call vector_store.upsert"
    );
    assert_eq!(output.vector_points_written, 0);
    assert_eq!(output.documents_prepared, 2);
    assert!(vectors.points("axon-test").await.is_empty());
}

/// `SourceRequest.limits.max_items` caps the number of package
/// pages/versions considered before diffing, so only the first `max_items`
/// items are prepared/vectorized even though the dump has more.
#[tokio::test]
async fn max_items_limit_caps_versions_prepared() {
    let dump_path = write_dump(valid_dump_json());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut capped_input = input(dump_path);
    capped_input.include_all_versions = true;
    capped_input.max_items = Some(1);

    let output = index_registry_source(capped_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(output.documents_prepared, 1);
}

use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use crate::test_support::committed_generation_payload;

use super::{
    LocalSourceIndexInput, LocalSourceSelectionPolicy, index_local_source,
    index_local_source_with_job,
};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x1111))
}

fn input(root: std::path::PathBuf) -> LocalSourceIndexInput {
    LocalSourceIndexInput {
        root,
        collection: "axon-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: None,
        vector_reservations: None,
        embed: true,
        route: None,
    }
}

fn input_with_reservations(root: std::path::PathBuf) -> LocalSourceIndexInput {
    let mut input = input(root);
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
async fn local_file_refresh_writes_vectors_then_commits_source_generation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 {\n    42\n}\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_local_source(input(path), &ledger, &embedder, &vectors)
        .await
        .unwrap();

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
    assert_eq!(source.counts.items_total, 1);
    assert_eq!(source.counts.documents_total, output.documents_prepared);
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors"): acquisition/prepare still runs (documents_prepared
/// stays non-zero) but neither the embedding provider nor `vector_store.upsert`
/// may be called.
#[tokio::test]
async fn embed_false_prepares_documents_but_writes_no_vectors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 {\n    42\n}\n")
        .await
        .unwrap();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(path);
    no_embed_input.embed = false;

    let output = index_local_source(no_embed_input, &ledger, &embedder, &vectors)
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
    assert!(output.documents_prepared >= 1);
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn local_source_job_emits_progress_events_for_pipeline_phases() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 {\n    42\n}\n")
        .await
        .unwrap();
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_local_source_with_job(input(path), &jobs, &ledger, &embedder, &vectors)
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
async fn local_source_job_records_provider_reservation_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    tokio::fs::write(&path, "pub fn answer() -> i32 {\n    42\n}\n")
        .await
        .unwrap();
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_local_source_with_job(
        input_with_reservations(path),
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

/// S2-routeplan-threading: a `RoutePlan` threaded in via
/// `LocalSourceIndexInput.route` (as `source::dispatch::dispatch_local` does
/// for every `index_source` call) must survive into the `SourcePlan` handed
/// to `LocalSourceAdapter` — specifically `validated_options` and
/// `credential_requirements` from the real router output, not the ad-hoc
/// empty ones `source_plan()` falls back to when `route` is `None`.
#[tokio::test]
async fn routed_plan_options_and_credentials_survive_into_adapter_run() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("lib.rs"), "pub fn f() {}\n")
        .await
        .unwrap();

    let mut request_options = MetadataMap::new();
    request_options.insert("respect_gitignore".to_string(), serde_json::json!(false));
    let route = RoutePlan {
        source: ResolvedSource::resolved(
            dir.path().to_string_lossy().to_string(),
            "local://placeholder",
            SourceId::new("src_local_placeholder"),
            SourceKind::Local,
            AdapterRef {
                name: "local".to_string(),
                version: "1".to_string(),
            },
            SourceScope::Directory,
            AuthorityLevel::UserPinned,
            1.0,
            "routed for test",
        ),
        adapter: AdapterRef {
            name: "local".to_string(),
            version: "1".to_string(),
        },
        scope: SourceScope::Directory,
        provider_requirements: Vec::new(),
        credential_requirements: vec![CredentialRequirement {
            credential_kind: CredentialKind::LocalConfig,
            secret_ref: None,
            required: false,
            reason: "routed marker credential".to_string(),
        }],
        execution_affinity: ExecutionAffinity::Worker,
        safety_class: SafetyClass::LocalFilesystem,
        option_schema_id: "adapter:local:options:v1".to_string(),
        validated_options: AdapterOptions {
            values: request_options,
        },
        chunking_hints: Vec::new(),
        parser_hints: Vec::new(),
        graph_fact_kinds: Vec::new(),
        watch_supported: true,
        refresh_supported: true,
    };

    let mut routed_input = input(dir.path().to_path_buf());
    routed_input.route = Some(route);

    let run = super::local_source_adapter::resolve_adapter_run(&routed_input)
        .await
        .expect("resolve_adapter_run should succeed");

    assert_eq!(
        run.plan
            .route
            .validated_options
            .values
            .get("respect_gitignore"),
        Some(&serde_json::json!(false)),
        "routed validated_options must survive into the adapter's SourcePlan"
    );
    assert_eq!(run.plan.route.credential_requirements.len(), 1);
    assert_eq!(
        run.plan.route.credential_requirements[0].credential_kind,
        CredentialKind::LocalConfig
    );
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

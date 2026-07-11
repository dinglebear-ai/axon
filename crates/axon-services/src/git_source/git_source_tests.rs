use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::test_support::committed_generation_payload;

use super::{GitSourceIndexInput, git_source_id, index_git_source, index_git_source_with_job};

const TARGET_URL: &str = "https://github.com/jmagar/fixture-repo";

fn job_id() -> JobId {
    JobId::new(Uuid::from_u128(0x2222))
}

fn fixture_repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axon-git-source-test-{}", Uuid::new_v4()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("README.md"), "# Fixture\n").unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        "pub fn answer() -> i32 {\n    42\n}\n",
    )
    .unwrap();
    // A .git directory that must be excluded from the walk.
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    dir
}

fn input(repo_root: PathBuf) -> GitSourceIndexInput {
    GitSourceIndexInput {
        target_url: TARGET_URL.to_string(),
        repo_root,
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
        route: None,
    }
}

fn input_with_reservations(repo_root: PathBuf) -> GitSourceIndexInput {
    let mut input = input(repo_root);
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
async fn git_repo_index_writes_vectors_then_commits_source_generation() {
    let repo = fixture_repo();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_git_source(input(repo.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert_eq!(output.documents_prepared, 2);
    assert!(output.chunks_prepared >= 2);
    assert!(output.vector_points_written >= 2);
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
    assert_eq!(source.source_kind, SourceKind::Git);
    assert_eq!(source.counts.items_total, 2);
    assert_eq!(source.counts.documents_total, output.documents_prepared);
    assert_eq!(
        git_source_id(&axon_adapters::git::parse_git_target(TARGET_URL).unwrap()),
        output.source_id
    );

    fs::remove_dir_all(&repo).ok();
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors"): acquisition/prepare still runs (documents_prepared
/// stays non-zero) but neither the embedding provider nor `vector_store.upsert`
/// may be called.
#[tokio::test]
async fn embed_false_prepares_documents_but_writes_no_vectors() {
    let repo = fixture_repo();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(repo.clone());
    no_embed_input.embed = false;

    let output = index_git_source(no_embed_input, &ledger, &embedder, &vectors)
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

    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn git_source_job_emits_progress_events_for_pipeline_phases() {
    let repo = fixture_repo();
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output =
        index_git_source_with_job(input(repo.clone()), &jobs, &ledger, &embedder, &vectors)
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

    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn git_source_job_records_provider_reservation_events() {
    let repo = fixture_repo();
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_git_source_with_job(
        input_with_reservations(repo.clone()),
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

    fs::remove_dir_all(&repo).ok();
}

#[tokio::test]
async fn unchanged_git_refresh_reuses_committed_generation_without_vector_work() {
    let repo = fixture_repo();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_git_source(input(repo.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_git_source(input(repo.clone()), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 0);
    assert_eq!(second.chunks_prepared, 0);
    assert_eq!(second.vector_points_written, 0);
    assert_eq!(embedder.calls().await.len(), embedding_calls);
    assert_eq!(vectors.calls().await, vector_calls);

    fs::remove_dir_all(&repo).ok();
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

/// S2-routeplan-threading: a `RoutePlan` threaded in via
/// `GitSourceIndexInput.route` (as `source::dispatch::dispatch_git` does for
/// every `index_source` call) must survive into the `SourcePlan` handed to
/// `GitSourceAdapter` — specifically `validated_options` and
/// `credential_requirements` from the real router output, not the ad-hoc
/// empty ones `source_plan()` falls back to when `route` is `None`.
#[test]
fn routed_plan_options_and_credentials_survive_into_adapter_run() {
    let repo = fixture_repo();

    let mut request_options = MetadataMap::new();
    request_options.insert("repo_root".to_string(), serde_json::json!("routed-marker"));
    let route = RoutePlan {
        source: ResolvedSource::resolved(
            TARGET_URL,
            TARGET_URL,
            SourceId::new("src_git_placeholder"),
            SourceKind::Git,
            AdapterRef {
                name: "git".to_string(),
                version: "1".to_string(),
            },
            SourceScope::Repo,
            AuthorityLevel::UserPinned,
            1.0,
            "routed for test",
        ),
        adapter: AdapterRef {
            name: "git".to_string(),
            version: "1".to_string(),
        },
        scope: SourceScope::Repo,
        provider_requirements: Vec::new(),
        credential_requirements: vec![CredentialRequirement {
            credential_kind: CredentialKind::OAuthToken,
            secret_ref: None,
            required: false,
            reason: "routed marker credential".to_string(),
        }],
        execution_affinity: ExecutionAffinity::Worker,
        safety_class: SafetyClass::LocalFilesystem,
        option_schema_id: "adapter:git:options:v1".to_string(),
        validated_options: AdapterOptions {
            values: request_options,
        },
        chunking_hints: Vec::new(),
        parser_hints: Vec::new(),
        graph_fact_kinds: Vec::new(),
        watch_supported: true,
        refresh_supported: true,
    };

    let mut routed_input = input(repo.clone());
    routed_input.route = Some(route);

    let run = super::git_source_adapter::resolve_adapter_run(&routed_input)
        .expect("resolve_adapter_run should succeed");

    assert_eq!(
        run.plan.route.validated_options.values.get("repo_root"),
        Some(&serde_json::json!("routed-marker")),
        "routed validated_options must survive into the adapter's SourcePlan"
    );
    assert_eq!(run.plan.route.credential_requirements.len(), 1);
    assert_eq!(
        run.plan.route.credential_requirements[0].credential_kind,
        CredentialKind::OAuthToken
    );

    fs::remove_dir_all(&repo).ok();
}

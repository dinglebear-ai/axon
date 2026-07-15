use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use crate::test_support::committed_generation_payload;

use super::{SessionsSourceIndexInput, index_sessions_source, index_sessions_source_with_job};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x2222))
}

fn input(sessions_root: std::path::PathBuf) -> SessionsSourceIndexInput {
    SessionsSourceIndexInput {
        sessions_root,
        provider: "claude".to_string(),
        session_id: "abc123".to_string(),
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
        project_filter: None,
    }
}

fn input_with_reservations(sessions_root: std::path::PathBuf) -> SessionsSourceIndexInput {
    let mut input = input(sessions_root);
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

/// A minimal Claude Code session export: one user turn, one assistant turn.
fn write_claude_fixture(dir: &std::path::Path) {
    std::fs::write(
        dir.join("session.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/j/proj","gitBranch":"main","timestamp":"2026-01-01T00:00:00Z","message":{"content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-x","content":[{"type":"text","text":"hi there"}]}}"#,
        ),
    )
    .unwrap();
}

/// Two session transcript files under the same `sessions_root`, so the
/// discovered manifest has two items for `max_items`/`embed` tests.
fn write_two_claude_fixtures(dir: &std::path::Path) {
    write_claude_fixture(dir);
    std::fs::write(
        dir.join("session2.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/j/proj","gitBranch":"main","timestamp":"2026-01-02T00:00:00Z","message":{"content":"second"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-02T00:00:01Z","message":{"model":"claude-x","content":[{"type":"text","text":"second reply"}]}}"#,
        ),
    )
    .unwrap();
}

#[test]
fn session_source_identity_includes_root_hash() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("one").join("same-name");
    let second = temp.path().join("two").join("same-name");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();

    let first_run =
        super::sessions_source_adapter::resolve_adapter_run(&input(first)).expect("first run");
    let second_run =
        super::sessions_source_adapter::resolve_adapter_run(&input(second)).expect("second run");

    assert_ne!(first_run.source_id, second_run.source_id);
    assert_ne!(
        first_run.plan.route.source.canonical_uri,
        second_run.plan.route.source.canonical_uri
    );
    assert!(
        first_run
            .plan
            .route
            .source
            .canonical_uri
            .starts_with("session://claude/abc123?root=")
    );
}

#[tokio::test]
async fn sessions_refresh_writes_vectors_then_commits_source_generation() {
    let dir = tempfile::tempdir().unwrap();
    write_claude_fixture(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_sessions_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
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
    // The bridge must sanitize adapter/preparer metadata to the shared vector
    // payload contract: forbidden absolute-path + non-allowlisted session
    // fields (and the transcript preparer's `segment_kind`) are dropped, while
    // the allowlisted `session_id` / `session_provider` and the `session`
    // source family survive. `session_provider` is `required:yes` per
    // `docs/pipeline-unification/sources/metadata-payload.md` (Session
    // Fields) — it must NOT be dropped (axon #298 contract fix).
    for point in vectors.points("axon-test").await {
        assert!(
            point.payload.get("session_workspace_path").is_none(),
            "forbidden absolute-path field must be stripped"
        );
        assert!(
            point.payload.get("session_agent").is_none(),
            "non-allowlisted session field must be stripped"
        );
        assert!(
            point.payload.get("segment_kind").is_none(),
            "preparer transcript chunk field must be stripped"
        );
        assert_eq!(point.payload["source_family"].as_str(), Some("session"));
        assert_eq!(point.payload["session_id"].as_str(), Some("abc123"));
        assert_eq!(
            point.payload["session_provider"].as_str(),
            Some("claude"),
            "required session_provider field must survive remap_to_vector_payload_contract"
        );
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

#[tokio::test]
async fn sessions_source_job_emits_progress_events_for_pipeline_phases() {
    let dir = tempfile::tempdir().unwrap();
    write_claude_fixture(dir.path());
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_sessions_source_with_job(
        input(dir.path().to_path_buf()),
        &jobs,
        &ledger,
        &embedder,
        &vectors,
    )
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
async fn sessions_source_job_records_provider_reservation_events() {
    let dir = tempfile::tempdir().unwrap();
    write_claude_fixture(dir.path());
    let jobs = FakeJobWatchStore::new();
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_sessions_source_with_job(
        input_with_reservations(dir.path().to_path_buf()),
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

#[tokio::test]
async fn sessions_refresh_reuses_committed_generation_without_vector_work() {
    let dir = tempfile::tempdir().unwrap();
    write_claude_fixture(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_sessions_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();
    assert!(first.vector_points_written >= 1);

    let second = index_sessions_source(
        input(dir.path().to_path_buf()),
        &ledger,
        &embedder,
        &vectors,
    )
    .await
    .unwrap();

    assert_eq!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 0);
    assert_eq!(second.chunks_prepared, 0);
    assert_eq!(second.vector_points_written, 0);
    // Only one embedding call across both runs: the unchanged refresh must not
    // re-embed.
    assert_eq!(embedder.calls().await.len(), 1);
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors"): session transcript files are still
/// discovered/prepared (documents_prepared stays non-zero) but neither the
/// embedding provider nor `vector_store.upsert` may be called.
#[tokio::test]
async fn embed_false_prepares_sessions_but_writes_no_vectors() {
    let dir = tempfile::tempdir().unwrap();
    write_two_claude_fixtures(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input(dir.path().to_path_buf());
    no_embed_input.embed = false;

    let output = index_sessions_source(no_embed_input, &ledger, &embedder, &vectors)
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

/// `SourceRequest.limits.max_items` caps the number of session transcript
/// files considered before diffing, so only the first `max_items` files are
/// prepared/vectorized even though the root has more.
#[tokio::test]
async fn max_items_limit_caps_sessions_prepared() {
    let dir = tempfile::tempdir().unwrap();
    write_two_claude_fixtures(dir.path());
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut capped_input = input(dir.path().to_path_buf());
    capped_input.max_items = Some(1);

    let output = index_sessions_source(capped_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(output.documents_prepared, 1);
}

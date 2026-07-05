use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;

use super::*;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;
use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;

#[derive(Default)]
struct CaptureEvents {
    events: Mutex<Vec<CodeSearchWatchEvent>>,
}

impl CodeSearchWatchEventSink for CaptureEvents {
    fn emit(&self, event: CodeSearchWatchEvent) {
        self.events.lock().expect("events").push(event);
    }
}

fn init_git_repo(root: &Path) {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "-q"])
        .status()
        .expect("git init");
}

fn target_context(
    source_jobs: Arc<FakeJobWatchStore>,
    vectors: Arc<FakeVectorStore>,
) -> ServiceContext {
    let cfg = Arc::new(axon_core::config::Config::test_default());
    ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime))
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs,
            Arc::new(FakeLedgerStore::new()),
            Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ))
}

#[test]
fn watcher_event_storm_coalesces_to_one_refresh() {
    let root = PathBuf::from("/workspace/repo");
    let mut dirty = BTreeMap::new();
    let old_enough = Instant::now() - Duration::from_secs(5);

    for _ in 0..100 {
        mark_dirty_root(&mut dirty, root.clone(), old_enough);
    }

    let refreshes_started = due_dirty_roots(&dirty, Duration::from_secs(1)).len();

    assert_eq!(refreshes_started, 1);
    assert_eq!(dirty.get(&root).map(|state| state.paths), Some(100));
}

#[tokio::test]
async fn watch_refresh_uses_target_local_source_runtime_when_available() {
    let repo = tempfile::tempdir().expect("repo");
    init_git_repo(repo.path());
    tokio::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .await
    .expect("source file");
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = target_context(source_jobs.clone(), vectors.clone());
    let events = CaptureEvents::default();

    refresh_code_search_watch_root(&ctx, &events, repo.path(), "file_change")
        .await
        .expect("target refresh");

    assert!(
        events
            .events
            .lock()
            .expect("events")
            .iter()
            .any(|event| matches!(event, CodeSearchWatchEvent::RefreshStarted { .. }))
    );
    let finished_generation = events
        .events
        .lock()
        .expect("events")
        .iter()
        .find_map(|event| match event {
            CodeSearchWatchEvent::RefreshFinished {
                target_source_generation,
                ..
            } => target_source_generation.clone(),
            _ => None,
        });
    assert!(
        finished_generation.is_some(),
        "target watch refresh should expose the committed source generation"
    );
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );

    let jobs = JobStore::list(
        source_jobs.as_ref(),
        JobListRequest {
            status: None,
            kind: Some(JobKind::Source),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .expect("jobs");
    assert_eq!(jobs.items.len(), 1);
    let job = &jobs.items[0];
    assert_eq!(job.kind, JobKind::Source);
    assert_eq!(job.status, LifecycleStatus::Completed);
    assert!(
        job.heartbeat.is_some(),
        "source job should record heartbeat"
    );

    let source_events = JobStore::events(
        source_jobs.as_ref(),
        JobEventListRequest {
            job_id: job.job_id,
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
    .expect("source events");
    assert!(!source_events.events.is_empty());
    assert!(
        source_events
            .events
            .iter()
            .all(|event| event.job_id == job.job_id)
    );
    assert!(
        source_events
            .events
            .iter()
            .any(|event| event.phase == PipelinePhase::Complete)
    );
    assert!(
        source_events
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.phase,
                    PipelinePhase::Embedding | PipelinePhase::Vectorizing
                )
            })
            .all(|event| progress_reservation_id(event).is_some()),
        "watch-triggered target refresh should expose provider reservation evidence"
    );
}

#[tokio::test]
async fn target_watch_refresh_returns_error_when_target_refresh_is_degraded() {
    let repo = tempfile::tempdir().expect("repo");
    init_git_repo(repo.path());
    tokio::fs::write(repo.path().join("bad.rs"), [0xff, 0xfe, 0xfd])
        .await
        .expect("bad file");
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = target_context(source_jobs, vectors);
    let events = CaptureEvents::default();

    let err = refresh_code_search_watch_root(&ctx, &events, repo.path(), "file_change")
        .await
        .expect_err("degraded target refresh should retry later");

    assert!(!err.to_string().is_empty(), "expected refresh error");
    let captured = events.events.lock().expect("events");
    assert!(
        captured
            .iter()
            .any(|event| matches!(event, CodeSearchWatchEvent::RefreshFailed { .. }))
    );
    assert!(
        !captured
            .iter()
            .any(|event| matches!(event, CodeSearchWatchEvent::RefreshFinished { .. }))
    );
}

fn progress_reservation_id(event: &JobEvent) -> Option<&str> {
    event
        .details
        .get("source_progress_event")?
        .get("reservation_id")?
        .as_str()
}

#[tokio::test]
async fn watch_refresh_preserves_legacy_backend_when_target_runtime_is_absent() {
    let repo = tempfile::tempdir().expect("repo");
    init_git_repo(repo.path());
    tokio::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .await
    .expect("source file");
    let cfg = Arc::new(axon_core::config::Config::test_default());
    let ctx = ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime));
    let events = CaptureEvents::default();

    let err = refresh_code_search_watch_root(&ctx, &events, repo.path(), "file_change")
        .await
        .expect_err("legacy refresh still requires sqlite runtime");
    assert!(err.to_string().contains("SQLite service runtime"));

    assert!(
        events
            .events
            .lock()
            .expect("events")
            .iter()
            .any(|event| matches!(event, CodeSearchWatchEvent::RefreshStarted { .. }))
    );
}

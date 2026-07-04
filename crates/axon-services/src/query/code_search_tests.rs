use super::*;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;
use crate::types::{CodeSearchCaller, CodeSearchFreshness, CodeSearchResult};
use axon_api::source::*;
use axon_code_index::{FreshnessWarning, ReindexProgress, ReindexProgressSink};
use axon_core::config::Config;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use axon_vectors::store::VectorStore;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingReindexProgress {
    events: Mutex<Vec<ReindexProgress>>,
}

impl ReindexProgressSink for RecordingReindexProgress {
    fn emit(&self, progress: ReindexProgress) {
        self.events.lock().expect("progress lock").push(progress);
    }
}

#[tokio::test]
async fn target_code_search_refresh_uses_local_source_runtime_when_available() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .expect("source file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg.clone(), service_jobs)
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs.clone(),
            ledger,
            embedder,
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let refreshed = refresh_code_search_index_with_progress(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        None,
    )
    .await
    .expect("target refresh");

    assert_eq!(refreshed.freshness.status, "fresh");
    assert!(refreshed.freshness.warning.is_none());
    assert!(refreshed.target_source_id.is_some());
    assert!(refreshed.target_source_generation.is_some());
    assert_eq!(refreshed.freshness.indexed_files, 1);
    assert_eq!(refreshed.freshness.removed_files, 0);
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
}

#[tokio::test]
async fn target_code_search_refresh_emits_progress_events_when_sink_is_present() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .expect("source file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg, service_jobs).with_target_local_source_runtime(
        TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ),
    );
    let progress = RecordingReindexProgress::default();

    let refreshed = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        Some(&progress),
    )
    .await
    .expect("target refresh");

    assert_eq!(refreshed.freshness.status, "fresh");
    let events = progress.events.lock().expect("progress lock").clone();
    assert!(matches!(
        events.first(),
        Some(ReindexProgress::Started { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(ReindexProgress::Finished { .. })
    ));
}

#[tokio::test]
async fn target_code_search_queries_committed_target_vectors_with_path_prefix() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::create_dir_all(repo.path().join("src")).expect("src dir");
    std::fs::create_dir_all(repo.path().join("docs")).expect("docs dir");
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn target_answer() -> i32 { 42 }\n",
    )
    .expect("source file");
    std::fs::write(
        repo.path().join("docs/notes.md"),
        "target_answer appears in docs but should be filtered out\n",
    )
    .expect("docs file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg.clone(), service_jobs)
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let refreshed = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("target refresh");
    assert_eq!(refreshed.freshness.status, "fresh");
    assert!(refreshed.freshness.warning.is_none());
    let mut stale_point = vectors
        .points(&cfg.collection)
        .await
        .into_iter()
        .find(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("src/lib.rs")
        })
        .expect("fresh src point");
    stale_point.point_id = VectorPointId::new("stale-src-lib");
    stale_point.chunk_id = ChunkId::new("stale-src-lib");
    stale_point.vector = vec![100.0; 8];
    stale_point
        .payload
        .insert("chunk_id".to_string(), serde_json::json!("stale-src-lib"));
    stale_point.payload.insert(
        "chunk_text".to_string(),
        serde_json::json!("stale generation"),
    );
    stale_point
        .payload
        .insert("source_generation".to_string(), serde_json::json!("old"));
    stale_point
        .payload
        .insert("committed_generation".to_string(), serde_json::json!("old"));
    let stale_batch_id = stale_point
        .payload
        .get("embedding_batch_id")
        .and_then(|value| value.as_str())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .map(BatchId::new)
        .expect("embedding batch id");
    vectors
        .upsert(VectorPointBatch {
            batch_id: stale_batch_id,
            collection: cfg.collection.clone(),
            points: vec![stale_point],
            model: "fake-embedding".to_string(),
            dimensions: 8,
            sparse_vectors: None,
            payload_indexes: Vec::new(),
        })
        .await
        .expect("stale point");

    let searched = code_search(
        &ctx,
        "target_answer",
        CodeSearchOptions {
            limit: 10,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: Some("src".to_string()),
            ensure_fresh: false,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("target code search");

    assert_eq!(searched.freshness.status, "skipped");
    assert!(searched.freshness.warning.is_none());
    assert_eq!(searched.results.len(), 1);
    assert_eq!(searched.results[0].file_path.as_deref(), Some("src/lib.rs"));
    assert_eq!(
        searched.results[0].snippet,
        "pub fn target_answer() -> i32 { 42 }"
    );
    assert_eq!(
        vectors.calls().await,
        vec![
            "ensure_collection",
            "upsert",
            "mark_generation_committed",
            "upsert",
            "search"
        ]
    );
}

#[tokio::test]
async fn target_code_search_errors_on_failed_refresh_but_can_query_committed_state_when_skipped() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn target_answer() -> i32 { 42 }\n",
    )
    .expect("source file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg, service_jobs).with_target_local_source_runtime(
        TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ),
    );

    refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("initial target refresh");
    std::fs::write(repo.path().join("bad.rs"), [0xff, 0xfe, 0xfd]).expect("bad source file");
    let progress = RecordingReindexProgress::default();

    let err = code_search_with_progress(
        &ctx,
        "target_answer",
        CodeSearchOptions {
            limit: 10,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: true,
            caller: CodeSearchCaller::Cli,
        },
        Some(&progress),
    )
    .await
    .expect_err("ensure_fresh target search should fail on refresh failure");
    assert!(
        err.to_string().contains("valid UTF-8"),
        "unexpected error: {err:#}"
    );
    let progress_events = progress.events.lock().expect("progress lock").clone();
    assert!(matches!(
        progress_events.first(),
        Some(ReindexProgress::Started { .. })
    ));
    assert!(
        !progress_events
            .iter()
            .any(|event| matches!(event, ReindexProgress::Finished { .. }))
    );

    let searched = code_search(
        &ctx,
        "target_answer",
        CodeSearchOptions {
            limit: 10,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: false,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("committed-state target search");
    assert_eq!(searched.freshness.status, "skipped");
    assert_eq!(searched.results.len(), 1);
    assert_eq!(searched.results[0].file_path.as_deref(), Some("lib.rs"));
}

#[tokio::test]
async fn target_code_search_refresh_reports_stale_when_runtime_missing() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .expect("source file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let ctx = ServiceContext::from_runtime(cfg, service_jobs);

    let refreshed = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("target refresh");

    assert_eq!(refreshed.freshness.status, "stale");
    assert_eq!(refreshed.freshness.indexed_files, 0);
    assert!(
        refreshed
            .freshness
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("target local source code-search refresh dependencies are not available")
    );
}
#[test]
fn code_search_result_marks_snippets_untrusted() {
    let result = CodeSearchResult {
        query: "find parser".to_string(),
        content_trust: "untrusted_local_code".to_string(),
        results: vec![],
        freshness: CodeSearchFreshness {
            status: "skipped".to_string(),
            warning: None,
            indexed_files: 0,
            removed_files: 0,
        },
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(
        value["content_trust"].as_str(),
        Some("untrusted_local_code")
    );
}

#[test]
fn code_search_missing_index_freshness_warns() {
    let freshness = code_search_missing_index_freshness(CodeSearchFreshness {
        status: "skipped".to_string(),
        warning: None,
        indexed_files: 0,
        removed_files: 0,
    });
    assert_eq!(freshness.status, "stale");
    assert_eq!(
        freshness.warning.as_deref(),
        Some("no committed code index; rerun without --no-freshness to build it")
    );
}

#[test]
fn code_search_freshness_marks_warning_branches_stale() {
    for warning in [
        FreshnessWarning::AlreadyRunning,
        FreshnessWarning::TimedOut { timeout_ms: 5000 },
        FreshnessWarning::Failed {
            error: "embed failed".to_string(),
        },
    ] {
        let freshness = code_search_freshness("fresh", Some(warning), 0, 0);
        assert_eq!(freshness.status, "stale");
        assert!(freshness.warning.is_some());
    }

    let skipped = code_search_freshness("skipped", None, 0, 0);
    assert_eq!(skipped.status, "skipped");
    assert!(skipped.warning.is_none());
}

#[test]
fn code_search_allowed_roots_error_does_not_leak_absolute_path() {
    let message = code_search_outside_allowed_roots_message();
    assert_eq!(
        message,
        "code_search cwd is outside AXON_CODE_SEARCH_ALLOWED_ROOTS"
    );
    assert!(!message.contains("/"));
}

#[tokio::test]
async fn code_search_resolution_errors_do_not_echo_probe_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("secret-checkout");
    let err = resolve_code_search_root(Some(&missing), CodeSearchCaller::Cli)
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(err, "code_search cwd could not be resolved");
    assert!(!err.contains(dir.path().to_string_lossy().as_ref()));
}

#[tokio::test]
async fn code_search_project_origin_is_checkout_scoped() {
    let a = tempfile::tempdir().expect("tempdir a");
    let b = tempfile::tempdir().expect("tempdir b");
    let origin_a = code_search_project_origin(a.path()).await;
    let origin_b = code_search_project_origin(b.path()).await;
    assert_ne!(origin_a, origin_b);
}

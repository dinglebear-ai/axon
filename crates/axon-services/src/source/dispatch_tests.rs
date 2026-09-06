//! Regression coverage for issue #298 WS-D (bead axon_rust-ruzox.4): full
//! `dispatch_session`/`dispatch_feed` round trips proving `SourceRequest.embed`
//! / `limits.max_items` reach the shared acquire-then-index path.
//! `session` needs no network (a local selector); `feed` is exercised against
//! a local `httpmock` server (loopback allowed via `LoopbackGuard`) so the
//! real `fetch_feed_to_file` acquire step runs too. `reddit`/`youtube`/
//! `registry` acquisition requires live OAuth credentials, a `yt-dlp`
//! subprocess, or a live public registry respectively — none mockable
//! offline, so their materialization behavior is covered in `axon-adapters`.

use axon_adapters::{
    ArtifactCandidateSink, FakeSourceAdapter, SourceAdapter, acquisition::MaterializedSource,
    feed::FeedSourceAdapter, local::LocalSourceAdapter,
};
use axon_api::source::{
    ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION, ARTIFACT_CANDIDATE_SCHEMA_VERSION, ApiError,
    ArtifactCandidate, ArtifactCandidateBatch, ArtifactCandidateId,
    ArtifactCandidateSinkCapability, ArtifactCandidateSinkResult, ArtifactCandidateSinkStatus,
    AuthScope, AuthSnapshot, LifecycleStatus, MetadataMap, ProviderId, SourceDocument,
    SourceEnrichment, SourceGenerationId, SourceItemKey, SourcePlan, SourceRequest, Timestamp,
};
use axon_core::http::LoopbackGuard;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use httpmock::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::source::{job_tracking, result_map};

use super::*;

const RSS_TWO_ITEMS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <link>https://example.com/</link>
  <item>
    <title>First Post</title>
    <link>https://example.com/a</link>
    <description>Hello world</description>
    <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
  </item>
  <item>
    <title>Second Post</title>
    <link>https://example.com/b</link>
    <description>Body two</description>
  </item>
</channel></rss>"#;

fn test_runtime_with_jobs(
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
    jobs: Arc<FakeJobWatchStore>,
) -> TargetLocalSourceRuntime {
    TargetLocalSourceRuntime::new(
        jobs,
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

fn test_runtime(
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
) -> TargetLocalSourceRuntime {
    test_runtime_with_jobs(vectors, ledger, Arc::new(FakeJobWatchStore::new()))
}

fn route_for(source: &str) -> axon_api::source::RoutePlan {
    crate::source::routing::resolve_source_route(&SourceRequest::new(source.to_string()))
        .expect("test source should route")
        .route
}

fn test_execution(source: &str) -> SourceExecutionContext {
    SourceExecutionContext::inline(SourceRequest::new(source), None)
}

/// Two session transcript files under one directory root, so the discovered
/// manifest has two items for `max_items`/`embed` assertions.
fn write_two_session_fixtures(dir: &std::path::Path) {
    std::fs::write(
        dir.join("session1.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/j/proj","gitBranch":"main","timestamp":"2026-01-01T00:00:00Z","message":{"content":"hello"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-x","content":[{"type":"text","text":"hi there"}]}}"#,
        ),
    )
    .unwrap();
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

fn claude_fixture_dir(home: &Path) -> PathBuf {
    let dir = home.join(".claude/projects/-home-j-proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_two_session_fixtures(&dir);
    dir
}

#[tokio::test]
async fn dispatch_local_denies_secret_like_path_before_bridge() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);
    let request = SourceRequest::local_path("./.env", false);
    let routed =
        crate::source::routing::resolve_source_route(&request).expect("local source should route");
    let mut snapshot = AuthSnapshot::default();
    snapshot.granted_scopes = vec![AuthScope::Read, AuthScope::Write, AuthScope::Local];
    let cfg = axon_core::config::Config::default();

    let result = dispatch_local(
        Arc::new(LocalSourceAdapter::new()),
        &cfg,
        &runtime,
        "./.env",
        "axon-test",
        "test-owner",
        Some(&snapshot),
        true,
        &routed.route,
        &test_execution("./.env"),
    )
    .await;
    let err = match result {
        Ok(_) => panic!("secret-like local paths should be denied before indexing"),
        Err(err) => err,
    };

    assert!(
        err.to_string().contains("security.local_secret_denied"),
        "expected secret-path denial, got: {err:?}"
    );
    assert!(
        vectors.points("axon-test").await.is_empty(),
        "denied local source must not write vectors"
    );
}

#[tokio::test]
async fn dispatch_session_embed_false_writes_no_vectors() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());

    let selector = format!("session:claude:{}", dir.display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let counts = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect("dispatch_session should succeed");

    assert_eq!(
        counts.documents_prepared, 2,
        "embed=false must still discover/prepare both session files"
    );
    assert_eq!(
        counts.vector_points_written, 0,
        "embed=false must not write any vectors"
    );
    assert!(
        vectors.points("axon-test").await.is_empty(),
        "embed=false must not call vector_store.upsert"
    );
    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
}

#[tokio::test]
async fn published_session_survives_lease_release_failures_as_degraded() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new().with_release_lease_failure());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors, ledger.clone());

    let selector = format!("session:claude:{}", dir.display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let counts = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect("published generation must survive lease release failures");

    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
    assert!(
        counts
            .warnings
            .iter()
            .any(|warning| { warning.code == "source.publish.finalizer_release_deferred" })
    );
    assert!(
        counts
            .warnings
            .iter()
            .any(|warning| { warning.code == "source.lease.release_deferred" })
    );
    let summary = ledger
        .get_source(counts.source_id.clone())
        .await
        .expect("source summary lookup")
        .expect("source summary");
    assert_eq!(summary.status, LifecycleStatus::CompletedDegraded);
    assert!(summary.last_refreshed_at.is_some());
}

#[tokio::test]
async fn published_session_defers_terminal_status_to_outer_orchestrator() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new());
    let runtime = test_runtime_with_jobs(vectors, ledger.clone(), jobs.clone());

    let selector = format!("session:claude:{}", dir.display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let counts = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect("published generation must survive terminal job status failure");

    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone())
    );
    assert!(
        jobs.recorded_status_updates(counts.job_id)
            .await
            .iter()
            .all(|update| !matches!(
                update.status,
                LifecycleStatus::Completed
                    | LifecycleStatus::CompletedDegraded
                    | LifecycleStatus::Failed
                    | LifecycleStatus::Canceled
            )),
        "the shared adapter pipeline must leave successful terminalization to graph/cleanup orchestration"
    );
    let summary = ledger
        .get_source(counts.source_id.clone())
        .await
        .expect("source summary lookup")
        .expect("source summary");
    assert_eq!(summary.status, LifecycleStatus::Completed);
    assert!(summary.last_refreshed_at.is_some());
}

#[tokio::test]
async fn committed_generation_survives_graph_audit_append_failure_as_degraded() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors, ledger.clone());

    let selector = format!("session:claude:{}", dir.display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let mut counts = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect("generation publication succeeds before post-publish auditing");

    let graph = axon_api::source::GraphWriteSummary {
        nodes_upserted: 1,
        edges_upserted: 0,
        evidence_records: 1,
        degraded: false,
    };
    let warning = job_tracking::track_graph_mutation(
        Some(Arc::new(
            FakeJobWatchStore::new().with_append_event_failure(),
        )),
        counts.job_id,
        None,
        &graph,
    )
    .await
    .expect("append failure produces retryable degradation");
    crate::source::record_post_publish_audit_warning(&runtime, &mut counts, Some(warning)).await;

    assert_eq!(
        ledger.committed_generation(&counts.source_id).await,
        Some(counts.generation.clone()),
        "an audit-only failure must not invalidate the current generation"
    );
    let summary = ledger
        .get_source(counts.source_id.clone())
        .await
        .expect("source summary lookup")
        .expect("source summary");
    assert_eq!(summary.status, LifecycleStatus::CompletedDegraded);
    let result = result_map::to_source_result_with_counts(
        route.source.source_kind,
        route.adapter,
        route.scope,
        route.source.canonical_uri,
        counts,
        graph,
        None,
    );
    assert_eq!(result.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(result.ledger.status, LifecycleStatus::CompletedDegraded);
    assert!(result.warnings[0].retryable);
}

#[tokio::test]
async fn dispatch_session_max_items_caps_documents_prepared() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors, ledger);

    let selector = format!("session:claude:{}", dir.display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let counts = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        true,
        Some(1),
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect("dispatch_session should succeed");

    assert_eq!(
        counts.documents_prepared, 1,
        "max_items=Some(1) must cap the discovered manifest before diffing"
    );
}

#[tokio::test]
async fn dispatch_session_rejects_paths_outside_provider_roots() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write_two_session_fixtures(outside.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger);

    let selector = format!("session:claude:{}", outside.path().display());
    let route = route_for(&selector);
    let execution = test_execution(&selector);
    let err = dispatch_session_with_roots(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        true,
        None,
        None,
        &route,
        &roots,
        &execution,
    )
    .await
    .expect_err("outside provider roots should be rejected");

    let error_chain = format!("{err:#}");
    assert!(
        error_chain.contains("outside provider roots"),
        "expected provider-root denial, got: {err:?}"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_feed_embed_false_writes_no_vectors() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let feed = server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("content-type", "application/rss+xml")
            .body(RSS_TWO_ITEMS);
    });
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());

    let source = server.url("/feed.xml");
    let route = route_for(&source);
    let execution = test_execution(&source);
    let counts = dispatch_feed(
        Arc::new(FeedSourceAdapter::new()),
        &runtime,
        &source,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
        &route,
        &execution,
    )
    .await
    .expect("dispatch_feed should succeed");

    feed.assert();
    assert_eq!(counts.documents_prepared, 2);
    assert_eq!(
        counts.vector_points_written, 0,
        "embed=false must not write any vectors"
    );
    assert!(vectors.points("axon-test").await.is_empty());
}

#[tokio::test]
async fn dispatch_feed_max_items_caps_documents_prepared() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let feed = server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("content-type", "application/rss+xml")
            .body(RSS_TWO_ITEMS);
    });
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors, ledger);

    let source = server.url("/feed.xml");
    let route = route_for(&source);
    let execution = test_execution(&source);
    let counts = dispatch_feed(
        Arc::new(FeedSourceAdapter::new()),
        &runtime,
        &source,
        "axon-test",
        "test-owner",
        None,
        true,
        Some(1),
        &route,
        &execution,
    )
    .await
    .expect("dispatch_feed should succeed");

    feed.assert();
    assert_eq!(
        counts.documents_prepared, 1,
        "max_items=Some(1) must cap the discovered manifest before diffing"
    );
}

#[derive(Clone)]
struct CandidateSourceAdapter {
    inner: FakeSourceAdapter,
}

impl CandidateSourceAdapter {
    fn new(inner: FakeSourceAdapter) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl SourceAdapter for CandidateSourceAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn version(&self) -> &'static str {
        self.inner.version()
    }

    async fn capabilities(
        &self,
    ) -> std::result::Result<axon_api::source::SourceAdapterCapability, ApiError> {
        self.inner.capabilities().await
    }

    async fn discover(
        &self,
        plan: &SourcePlan,
    ) -> std::result::Result<axon_api::source::SourceManifest, ApiError> {
        self.inner.discover(plan).await
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &axon_api::source::SourceManifestDiff,
    ) -> std::result::Result<axon_api::source::SourceAcquisition, ApiError> {
        self.inner.acquire(plan, diff).await
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: axon_api::source::SourceAcquisition,
    ) -> std::result::Result<axon_api::source::StageExecutionResult<Vec<SourceDocument>>, ApiError>
    {
        self.inner.normalize(plan, acquisition).await
    }

    async fn artifact_candidates(
        &self,
        plan: &SourcePlan,
        generation: &SourceGenerationId,
        documents: &[SourceDocument],
        _enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
    ) -> std::result::Result<Vec<ArtifactCandidate>, ApiError> {
        Ok(documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let mut manifest_metadata = MetadataMap::new();
                manifest_metadata.insert(
                    "axonSourceItemKey".to_string(),
                    serde_json::json!(document.source_item_key.0.clone()),
                );
                ArtifactCandidate {
                    schema_version: ARTIFACT_CANDIDATE_SCHEMA_VERSION.to_string(),
                    id: ArtifactCandidateId::from(format!("cand_pipeline_{index}")),
                    canonical_source_uri: document.canonical_uri.clone(),
                    source_provider: "axon".to_string(),
                    observed_at: Timestamp("2026-08-19T14:00:00Z".to_string()),
                    repository: None,
                    source_ref: None,
                    source_path: document.path.clone(),
                    kind_hints: vec!["skill".to_string()],
                    observed_files: Vec::new(),
                    manifest_metadata,
                    content_digests: Vec::new(),
                    discovery_evidence: MetadataMap::new(),
                    popularity_signals: MetadataMap::new(),
                    license_evidence: MetadataMap::new(),
                    crawl_generation_id: Some(generation.0.clone()),
                    crawl_job_id: Some(plan.job_id.0.to_string()),
                    warnings: Vec::new(),
                }
            })
            .collect())
    }
}

#[derive(Clone)]
struct CommitAwareCandidateSink {
    ledger: Arc<FakeLedgerStore>,
    deliveries: Arc<Mutex<Vec<(ArtifactCandidateBatch, bool)>>>,
}

#[derive(Clone)]
struct RetryThenAcceptCandidateSink {
    attempts: Arc<std::sync::atomic::AtomicUsize>,
    fail_count: usize,
}

#[derive(Clone)]
struct DisabledCandidateSink {
    attempts: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl ArtifactCandidateSink for DisabledCandidateSink {
    async fn submit(
        &self,
        batch: ArtifactCandidateBatch,
    ) -> std::result::Result<ArtifactCandidateSinkResult, ApiError> {
        self.attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(ArtifactCandidateSinkResult {
            status: ArtifactCandidateSinkStatus::Disabled,
            attempted: batch.candidates.len() as u64,
            accepted: 0,
            rejected: 0,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> std::result::Result<ArtifactCandidateSinkCapability, ApiError> {
        Ok(ArtifactCandidateSinkCapability {
            name: "disabled-test".to_string(),
            version: "1".to_string(),
            contract_versions: vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()],
            max_batch_size: 64,
            supports_idempotency: true,
        })
    }
}

#[async_trait::async_trait]
impl ArtifactCandidateSink for RetryThenAcceptCandidateSink {
    async fn submit(
        &self,
        batch: ArtifactCandidateBatch,
    ) -> std::result::Result<ArtifactCandidateSinkResult, ApiError> {
        let attempt = self
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if attempt <= self.fail_count {
            return Err(ApiError::new(
                "provider.artifact_candidate.unavailable",
                axon_error::ErrorStage::Publishing,
                "synthetic retryable outage",
            ));
        }
        let attempted = batch.candidates.len() as u64;
        Ok(ArtifactCandidateSinkResult {
            status: ArtifactCandidateSinkStatus::Accepted,
            attempted,
            accepted: attempted,
            rejected: 0,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> std::result::Result<ArtifactCandidateSinkCapability, ApiError> {
        Ok(ArtifactCandidateSinkCapability {
            name: "retry-then-accept-test".to_string(),
            version: "1".to_string(),
            contract_versions: vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()],
            max_batch_size: 64,
            supports_idempotency: true,
        })
    }
}

impl CommitAwareCandidateSink {
    fn new(ledger: Arc<FakeLedgerStore>) -> Self {
        Self {
            ledger,
            deliveries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn deliveries(&self) -> Vec<(ArtifactCandidateBatch, bool)> {
        self.deliveries
            .lock()
            .expect("candidate delivery mutex poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl ArtifactCandidateSink for CommitAwareCandidateSink {
    async fn submit(
        &self,
        batch: ArtifactCandidateBatch,
    ) -> std::result::Result<ArtifactCandidateSinkResult, ApiError> {
        let committed = self
            .ledger
            .committed_generation(&batch.source_id)
            .await
            .is_some_and(|generation| generation == batch.generation);
        let attempted = batch.candidates.len() as u64;
        self.deliveries
            .lock()
            .expect("candidate delivery mutex poisoned")
            .push((batch, committed));
        Ok(ArtifactCandidateSinkResult {
            status: ArtifactCandidateSinkStatus::Accepted,
            attempted,
            accepted: attempted,
            rejected: 0,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> std::result::Result<ArtifactCandidateSinkCapability, ApiError> {
        Ok(ArtifactCandidateSinkCapability {
            name: "commit-aware-test".to_string(),
            version: "1".to_string(),
            contract_versions: vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()],
            max_batch_size: 64,
            supports_idempotency: true,
        })
    }
}

/// Stamp the adapter-owned vector payload fields the point builder requires,
/// the way real adapters (e.g. `local`) do in `normalize` —
/// `FakeSourceAdapter` leaves document metadata empty, which fails vector
/// payload validation when `embed = true`.
fn stamp_required_payload_metadata(
    plan: &SourcePlan,
    result: &mut axon_api::source::StageExecutionResult<Vec<SourceDocument>>,
) {
    for document in &mut result.data {
        let metadata = &mut document.metadata;
        metadata.insert("source_family".to_string(), serde_json::json!("code"));
        metadata.insert(
            "source_kind".to_string(),
            serde_json::json!(plan.route.source.source_kind),
        );
        metadata.insert(
            "source_adapter".to_string(),
            serde_json::json!(plan.route.adapter.name),
        );
        metadata.insert(
            "source_scope".to_string(),
            serde_json::json!(plan.route.scope),
        );
        metadata.insert(
            "item_canonical_uri".to_string(),
            serde_json::json!(document.canonical_uri.clone()),
        );
        metadata.insert(
            "committed_generation".to_string(),
            serde_json::json!("uncommitted"),
        );
        metadata.insert("visibility".to_string(), serde_json::json!("internal"));
        metadata.insert("redaction_status".to_string(), serde_json::json!("clean"));
    }
}

/// [`FakeSourceAdapter`] plus the payload metadata stamp above, so
/// `embed = true` pipelines run end-to-end against the fakes.
struct StampingSourceAdapter {
    inner: FakeSourceAdapter,
}

#[async_trait::async_trait]
impl SourceAdapter for StampingSourceAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn version(&self) -> &'static str {
        self.inner.version()
    }

    async fn capabilities(
        &self,
    ) -> std::result::Result<axon_api::source::SourceAdapterCapability, ApiError> {
        self.inner.capabilities().await
    }

    async fn discover(
        &self,
        plan: &SourcePlan,
    ) -> std::result::Result<axon_api::source::SourceManifest, ApiError> {
        self.inner.discover(plan).await
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &axon_api::source::SourceManifestDiff,
    ) -> std::result::Result<axon_api::source::SourceAcquisition, ApiError> {
        self.inner.acquire(plan, diff).await
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: axon_api::source::SourceAcquisition,
    ) -> std::result::Result<axon_api::source::StageExecutionResult<Vec<SourceDocument>>, ApiError>
    {
        let mut result = self.inner.normalize(plan, acquisition).await?;
        stamp_required_payload_metadata(plan, &mut result);
        Ok(result)
    }
}

#[tokio::test]
async fn removal_only_recrawl_publishes_generation_and_retires_removed_docs() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().to_string_lossy().to_string();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());

    let full = StampingSourceAdapter {
        inner: FakeSourceAdapter::new(route.adapter.clone())
            .with_item(
                "kept.md",
                axon_api::source::ContentKind::Markdown,
                "# Kept\nbody\n",
            )
            .with_item(
                "removed.md",
                axon_api::source::ContentKind::Markdown,
                "# Removed\nbody\n",
            ),
    };
    let first = dispatch_materialized(
        &runtime,
        &full,
        family_source_plan(&source, &route, true, None, None),
        "axon-test",
        "test-owner",
        None,
        &test_execution(&source),
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect("initial two-item generation succeeds");
    assert_eq!(first.documents_prepared, 2);
    assert!(first.vector_points_written > 0);

    let shrunk = StampingSourceAdapter {
        inner: FakeSourceAdapter::new(route.adapter.clone()).with_item(
            "kept.md",
            axon_api::source::ContentKind::Markdown,
            "# Kept\nbody\n",
        ),
    };
    let second = dispatch_materialized(
        &runtime,
        &shrunk,
        family_source_plan(&source, &route, true, None, None),
        "axon-test",
        "test-owner",
        None,
        &test_execution(&source),
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect(
        "a removal-only recrawl has zero changed acquisition batches and must \
         publish the shrunk generation instead of failing (H1)",
    );

    assert_eq!(second.removed, 1);
    assert_ne!(second.generation, first.generation);
    assert_eq!(
        ledger.committed_generation(&second.source_id).await,
        Some(second.generation.clone()),
        "the removal-only generation must commit"
    );
    let points = vectors.points("axon-test").await;
    let item_key = |point: &axon_api::source::VectorPoint| {
        point
            .payload
            .get("source_item_key")
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };
    // Live points carry `retired_epoch: null`; retirement stamps a number.
    let retired = |point: &axon_api::source::VectorPoint| {
        point
            .payload
            .get("retired_epoch")
            .is_some_and(|value| !value.is_null())
    };
    let removed_points = points
        .iter()
        .filter(|point| item_key(point).as_deref() == Some("removed.md"))
        .collect::<Vec<_>>();
    assert!(
        !removed_points.is_empty() && removed_points.iter().all(|point| retired(point)),
        "every vector point of the removed document must be retired"
    );
    assert!(
        points
            .iter()
            .any(|point| { item_key(point).as_deref() == Some("kept.md") && !retired(point) }),
        "the kept document must stay live in the published generation"
    );
}

/// Adapter whose N-th `normalize` call parks forever after signalling the
/// test, so cancellation can be delivered mid-generation with the first
/// batch's vectors already upserted.
struct BlockingNormalizeAdapter {
    inner: FakeSourceAdapter,
    normalize_calls: std::sync::atomic::AtomicUsize,
    block_on_call: usize,
    entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[async_trait::async_trait]
impl SourceAdapter for BlockingNormalizeAdapter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn version(&self) -> &'static str {
        self.inner.version()
    }

    async fn capabilities(
        &self,
    ) -> std::result::Result<axon_api::source::SourceAdapterCapability, ApiError> {
        self.inner.capabilities().await
    }

    async fn discover(
        &self,
        plan: &SourcePlan,
    ) -> std::result::Result<axon_api::source::SourceManifest, ApiError> {
        self.inner.discover(plan).await
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &axon_api::source::SourceManifestDiff,
    ) -> std::result::Result<axon_api::source::SourceAcquisition, ApiError> {
        self.inner.acquire(plan, diff).await
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: axon_api::source::SourceAcquisition,
    ) -> std::result::Result<axon_api::source::StageExecutionResult<Vec<SourceDocument>>, ApiError>
    {
        let call = self
            .normalize_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if call == self.block_on_call {
            if let Some(entered) = self.entered.lock().expect("entered mutex poisoned").take() {
                let _ = entered.send(());
            }
            std::future::pending::<()>().await;
        }
        let mut result = self.inner.normalize(plan, acquisition).await?;
        stamp_required_payload_metadata(plan, &mut result);
        Ok(result)
    }
}

#[tokio::test]
async fn cancellation_mid_generation_cleans_vectors_and_fails_generation() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().to_string_lossy().to_string();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());

    // Two acquisition batches (65 items > the 64-item streaming batch bound):
    // the first batch upserts its vectors, the second parks in `normalize`
    // while the test delivers cancellation.
    let mut inner = FakeSourceAdapter::new(route.adapter.clone());
    for index in 0..65 {
        inner = inner.with_item(
            format!("doc-{index:03}.md"),
            axon_api::source::ContentKind::Markdown,
            format!("# Doc {index}\nbody\n"),
        );
    }
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let adapter = BlockingNormalizeAdapter {
        inner,
        normalize_calls: std::sync::atomic::AtomicUsize::new(0),
        block_on_call: 2,
        entered: Mutex::new(Some(entered_tx)),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let execution = test_execution(&source).with_cancellation(cancel.clone());

    let run = dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, true, None, None),
        "axon-test",
        "test-owner",
        None,
        &execution,
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    );
    let (result, ()) = tokio::join!(run, async {
        entered_rx
            .await
            .expect("second batch normalization must start");
        cancel.cancel();
    });

    let error = result.expect_err("a canceled run must surface an error");
    assert!(
        format!("{error:#}").contains("canceled"),
        "expected a cancellation error, got: {error:#}"
    );
    assert!(
        ledger
            .committed_generation(&route.source.source_id)
            .await
            .is_none(),
        "a canceled generation must not commit"
    );
    let generation = ledger
        .generation(&route.source.source_id, &SourceGenerationId::new("gen_1"))
        .await
        .expect("uncommitted generation row");
    assert_eq!(
        generation.status,
        LifecycleStatus::Failed,
        "cancel must mark the uncommitted generation row failed (M3)"
    );
    assert!(
        vectors.points("axon-test").await.is_empty(),
        "cancel must clean the uncommitted generation's already-upserted vectors (M3)"
    );
    let summary = ledger
        .get_source(route.source.source_id.clone())
        .await
        .expect("source summary lookup")
        .expect("source summary");
    assert_eq!(summary.status, LifecycleStatus::Failed);
}

#[tokio::test]
async fn artifact_candidates_are_delivered_after_commit_and_not_replayed_when_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().to_string_lossy().to_string();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new());
    let sink = Arc::new(CommitAwareCandidateSink::new(ledger.clone()));
    let runtime = test_runtime_with_jobs(vectors.clone(), ledger.clone(), jobs)
        .with_artifact_candidate_sink(sink.clone());
    let adapter =
        CandidateSourceAdapter::new(FakeSourceAdapter::new(route.adapter.clone()).with_item(
            "SKILL.md",
            axon_api::source::ContentKind::Markdown,
            "# Demo skill",
        ));

    let first_execution = test_execution(&source);
    let first = dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, false, None, None),
        "axon-test",
        "test-owner",
        None,
        &first_execution,
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect("first artifact-aware source generation succeeds");

    assert_eq!(first.documents_prepared, 1);
    assert_eq!(first.vector_points_written, 0);
    assert!(vectors.points("axon-test").await.is_empty());
    assert_eq!(
        ledger.committed_generation(&first.source_id).await,
        Some(first.generation.clone())
    );
    let deliveries = sink.deliveries();
    assert_eq!(deliveries.len(), 1);
    assert!(
        deliveries[0].1,
        "candidate sink ran before generation commit"
    );
    assert_eq!(deliveries[0].0.generation, first.generation);
    assert_eq!(deliveries[0].0.candidates.len(), 1);
    assert_eq!(
        deliveries[0].0.candidates[0].schema_version,
        ARTIFACT_CANDIDATE_SCHEMA_VERSION
    );

    let second_execution = test_execution(&source);
    let second = dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, false, None, None),
        "axon-test",
        "test-owner",
        None,
        &second_execution,
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect("unchanged artifact-aware refresh succeeds");

    assert_eq!(second.generation, first.generation);
    assert_eq!(
        sink.deliveries().len(),
        1,
        "unchanged refresh must not replay ArtifactCandidate delivery"
    );
}

#[tokio::test]
async fn durable_candidate_outbox_retries_autonomously_then_deletes_on_acceptance() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source").to_string_lossy().to_string();
    std::fs::create_dir_all(&source).unwrap();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new());
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sink = Arc::new(RetryThenAcceptCandidateSink {
        attempts: Arc::clone(&attempts),
        fail_count: 2,
    });
    let outbox = Arc::new(
        crate::artifact_candidate_outbox::ArtifactCandidateOutbox::new(root.path().join("outbox")),
    );
    let mut runtime = test_runtime_with_jobs(vectors, ledger, jobs);
    runtime.artifact_candidate_outbox = Some(Arc::clone(&outbox));
    let runtime = runtime.with_artifact_candidate_sink(sink);
    let adapter =
        CandidateSourceAdapter::new(FakeSourceAdapter::new(route.adapter.clone()).with_item(
            "SKILL.md",
            axon_api::source::ContentKind::Markdown,
            "# Demo skill",
        ));

    dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, false, None, None),
        "axon-test",
        "test-owner",
        None,
        &test_execution(&source),
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect("source generation succeeds while candidate sink retries");

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if attempts.load(std::sync::atomic::Ordering::SeqCst) == 3
                && outbox.pending().await.expect("read outbox").is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("autonomous retry drain completes");
}

#[tokio::test]
async fn disabled_candidate_sink_preserves_durable_outbox_delivery() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source").to_string_lossy().to_string();
    std::fs::create_dir_all(&source).unwrap();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new());
    let outbox = Arc::new(
        crate::artifact_candidate_outbox::ArtifactCandidateOutbox::new(root.path().join("outbox")),
    );
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut runtime = test_runtime_with_jobs(vectors, ledger, jobs);
    runtime.artifact_candidate_outbox = Some(Arc::clone(&outbox));
    let runtime = runtime.with_artifact_candidate_sink(Arc::new(DisabledCandidateSink {
        attempts: Arc::clone(&attempts),
    }));
    let adapter =
        CandidateSourceAdapter::new(FakeSourceAdapter::new(route.adapter.clone()).with_item(
            "SKILL.md",
            axon_api::source::ContentKind::Markdown,
            "# Demo skill",
        ));

    dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, false, None, None),
        "axon-test",
        "test-owner",
        None,
        &test_execution(&source),
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await
    .expect("source generation succeeds while candidate sink is disabled");

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while attempts.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disabled sink was attempted");
    assert_eq!(
        outbox.pending().await.expect("read outbox").len(),
        1,
        "disabled delivery must remain durable for a later configured sink"
    );
}

#[tokio::test]
async fn failed_generation_never_delivers_artifact_candidates() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().to_string_lossy().to_string();
    let route = route_for(&source);
    let ledger = Arc::new(FakeLedgerStore::new().with_publish_generation_failure());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new());
    let sink = Arc::new(CommitAwareCandidateSink::new(ledger.clone()));
    let runtime = test_runtime_with_jobs(vectors, ledger.clone(), jobs)
        .with_artifact_candidate_sink(sink.clone());
    let adapter =
        CandidateSourceAdapter::new(FakeSourceAdapter::new(route.adapter.clone()).with_item(
            "SKILL.md",
            axon_api::source::ContentKind::Markdown,
            "# Demo skill",
        ));

    let result = dispatch_materialized(
        &runtime,
        &adapter,
        family_source_plan(&source, &route, false, None, None),
        "axon-test",
        "test-owner",
        None,
        &test_execution(&source),
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await;

    assert!(
        result.is_err(),
        "generation publication failure must surface"
    );
    assert!(
        ledger
            .committed_generation(&route.source.source_id)
            .await
            .is_none(),
        "failed generation must not commit"
    );
    assert!(
        sink.deliveries().is_empty(),
        "failed generation leaked a ghost candidate to the sink"
    );
}

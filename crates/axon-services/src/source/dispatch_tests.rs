//! Regression coverage for issue #298 WS-D (bead axon_rust-ruzox.4): full
//! `dispatch_session`/`dispatch_feed` round trips proving `SourceRequest.embed`
//! / `limits.max_items` reach the shared acquire-then-index path.
//! `session` needs no network (a local selector); `feed` is exercised against
//! a local `httpmock` server (loopback allowed via `LoopbackGuard`) so the
//! real `fetch_feed_to_file` acquire step runs too. `reddit`/`youtube`/
//! `registry` acquisition requires live OAuth credentials, a `yt-dlp`
//! subprocess, or a live public registry respectively — none mockable
//! offline, so their materialization behavior is covered in `axon-adapters`.

use axon_adapters::{feed::FeedSourceAdapter, local::LocalSourceAdapter};
use axon_api::source::{AuthScope, AuthSnapshot, LifecycleStatus, ProviderId, SourceRequest};
use axon_core::http::LoopbackGuard;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;
use httpmock::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

fn test_runtime(
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
) -> TargetLocalSourceRuntime {
    test_runtime_with_jobs(vectors, ledger, Arc::new(FakeJobWatchStore::new()))
}

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
async fn published_session_survives_terminal_status_failure_as_degraded() {
    let home = tempfile::tempdir().unwrap();
    let dir = claude_fixture_dir(home.path());
    let roots = crate::sessions::SessionRoots::for_home(home.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let jobs = Arc::new(FakeJobWatchStore::new().with_terminal_status_failure());
    let runtime = test_runtime_with_jobs(vectors, ledger.clone(), jobs);

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
        counts
            .warnings
            .iter()
            .any(|warning| { warning.code == "source.job.terminal_status_deferred" })
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

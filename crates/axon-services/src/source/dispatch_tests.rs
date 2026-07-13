//! Regression coverage for issue #298 WS-D (bead axon_rust-ruzox.4): full
//! `dispatch_session`/`dispatch_feed` round trips proving `SourceRequest.embed`
//! / `limits.max_items` reach the real acquire-then-index path, not just the
//! pure `*SourceIndexInput` builders covered by `dispatch/index_inputs_tests.rs`.
//! `session` needs no network (a local selector); `feed` is exercised against
//! a local `httpmock` server (loopback allowed via `LoopbackGuard`) so the
//! real `fetch_feed_to_file` acquire step runs too. `reddit`/`youtube`/
//! `registry` acquisition requires live OAuth credentials, a `yt-dlp`
//! subprocess, or a live public registry respectively — none mockable
//! offline — so their threading is covered by `index_inputs_tests.rs` plus
//! the existing bridge-level `embed`/`max_items` tests in
//! `reddit_source_tests.rs`/`youtube_source_tests.rs`/`registry_source_tests.rs`.

use axon_api::source::ProviderId;
use axon_core::http::LoopbackGuard;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use httpmock::prelude::*;
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
    TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

/// Two session transcript files under one directory root, so the discovered
/// manifest has two items for `max_items`/`embed` assertions (mirrors
/// `sessions_source_tests.rs::write_two_claude_fixtures`).
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

#[tokio::test]
async fn dispatch_session_embed_false_writes_no_vectors() {
    let dir = tempfile::tempdir().unwrap();
    write_two_session_fixtures(dir.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors.clone(), ledger.clone());

    let selector = format!("session:claude:{}", dir.path().display());
    let counts = dispatch_session(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        false,
        None,
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
async fn dispatch_session_max_items_caps_documents_prepared() {
    let dir = tempfile::tempdir().unwrap();
    write_two_session_fixtures(dir.path());
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let runtime = test_runtime(vectors, ledger);

    let selector = format!("session:claude:{}", dir.path().display());
    let counts = dispatch_session(
        &runtime,
        &selector,
        "axon-test",
        "test-owner",
        None,
        true,
        Some(1),
    )
    .await
    .expect("dispatch_session should succeed");

    assert_eq!(
        counts.documents_prepared, 1,
        "max_items=Some(1) must cap the discovered manifest before diffing"
    );
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

    let counts = dispatch_feed(
        &runtime,
        &server.url("/feed.xml"),
        "axon-test",
        "test-owner",
        None,
        false,
        None,
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

    let counts = dispatch_feed(
        &runtime,
        &server.url("/feed.xml"),
        "axon-test",
        "test-owner",
        None,
        true,
        Some(1),
    )
    .await
    .expect("dispatch_feed should succeed");

    feed.assert();
    assert_eq!(
        counts.documents_prepared, 1,
        "max_items=Some(1) must cap the discovered manifest before diffing"
    );
}

//! Regression coverage for issue #298 WS-D (bead axon_rust-ruzox.4): every
//! `dispatch_{feed,reddit,youtube,registry,session}` function used to
//! hardcode `embed: true, max_items: None` in its `*SourceIndexInput`
//! literal, silently dropping `SourceRequest.embed`/`limits.max_items` for
//! these five families. These tests build each family's index input directly
//! and assert the caller-supplied `embed`/`max_items` values — not the old
//! hardcoded defaults — land on the constructed struct.

use axon_api::source::ProviderId;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

use super::*;

fn test_runtime() -> TargetLocalSourceRuntime {
    TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        Arc::new(FakeVectorStore::new("fake-vector")),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

#[test]
fn feed_index_input_threads_embed_and_max_items() {
    let runtime = test_runtime();
    let input = feed_index_input(
        &runtime,
        PathBuf::from("/tmp/axon-feeds/example.xml"),
        "axon-test",
        "test-owner",
        None,
        false,
        Some(3),
    );
    assert!(!input.embed, "embed=false must not become true");
    assert_eq!(input.max_items, Some(3), "max_items must not become None");
}

#[test]
fn reddit_index_input_threads_embed_and_max_items() {
    let runtime = test_runtime();
    let input = reddit_index_input(
        &runtime,
        "r/rust",
        PathBuf::from("/tmp/axon-reddit/example.json"),
        "axon-test",
        "test-owner",
        None,
        false,
        Some(3),
    );
    assert!(!input.embed, "embed=false must not become true");
    assert_eq!(input.max_items, Some(3), "max_items must not become None");
}

#[test]
fn youtube_index_input_threads_embed_and_max_items() {
    let runtime = test_runtime();
    let input = youtube_index_input(
        &runtime,
        "https://youtube.com/watch?v=dQw4w9WgXcQ",
        PathBuf::from("/tmp/axon-youtube/example.json"),
        "axon-test",
        "test-owner",
        None,
        false,
        Some(3),
    );
    assert!(!input.embed, "embed=false must not become true");
    assert_eq!(input.max_items, Some(3), "max_items must not become None");
}

#[test]
fn registry_index_input_threads_embed_and_max_items() {
    let runtime = test_runtime();
    let input = registry_index_input(
        &runtime,
        PathBuf::from("/tmp/axon-registry/example.json"),
        "axon-test",
        "test-owner",
        None,
        false,
        Some(3),
    );
    assert!(!input.embed, "embed=false must not become true");
    assert_eq!(input.max_items, Some(3), "max_items must not become None");
}

#[test]
fn session_index_input_threads_embed_and_max_items() {
    let runtime = test_runtime();
    let input = session_index_input(
        &runtime,
        PathBuf::from("/tmp/axon-sessions/claude"),
        "claude".to_string(),
        "abc123".to_string(),
        "axon-test",
        "test-owner",
        None,
        false,
        Some(3),
        Some("axon".to_string()),
    );
    assert!(!input.embed, "embed=false must not become true");
    assert_eq!(input.max_items, Some(3), "max_items must not become None");
    assert_eq!(input.project_filter.as_deref(), Some("axon"));
}

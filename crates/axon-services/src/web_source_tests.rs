//! `index_web_source` integration tests.
//!
//! Issue #298 Wave 1b retired the `manifest.jsonl`/`markdown_root` disk
//! handoff — `WebSourceAdapter` now fetches through `FetchProvider`/
//! `RenderProvider`. These tests use `axon_adapters::boundary::
//! FakeAdapterProviders` and drive single-URL (`SourceScope::Page`) inputs,
//! since only `Site`/`Docs` scope's `discover` enumerates *multiple* URLs —
//! and it does so via a real `axon-crawl` engine crawl (issue #298 Wave 1b),
//! which cannot be driven hermetically here (Spider's own SSRF blacklist
//! unconditionally blocks loopback addresses, independent of the
//! `LoopbackGuard` test-util bypass `validate_url` honors — see
//! `axon_core::http::ssrf::ssrf_blacklist_compact_strings`). The multi-page
//! fixture tests this file used to carry (mixed unchanged/changed diffing,
//! >1-batch vectorization) were removed for this reason; see issue #298 Wave
//! 2 follow-ups for restoring hermetic multi-page coverage (e.g. an
//! injectable discovery function, or a test-only SSRF bypass for Spider's
//! blacklist).

use std::sync::Arc;

use axon_adapters::boundary::FakeAdapterProviders;
use axon_api::source::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;

use crate::test_support::committed_generation_payload;

use super::{WebSourceIndexInput, index_web_source};

fn job_id() -> JobId {
    JobId::new(uuid::Uuid::from_u128(0x29812))
}

fn input() -> WebSourceIndexInput {
    let providers = Arc::new(FakeAdapterProviders::new());
    WebSourceIndexInput {
        source: "https://example.com/docs?utm_source=noise".to_string(),
        scope: SourceScope::Page,
        map_urls: Vec::new(),
        crawl_options: MetadataMap::new(),
        output: OutputPolicy::default(),
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        attempt: 1,
        embed: true,
        fetch_provider: providers.clone(),
        render_provider: providers,
        artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        document_cache: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        event_store: None,
    }
}

#[tokio::test]
async fn web_source_refresh_writes_vectors_then_commits_generation() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let output = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(embedder.calls().await.len(), 1);
    assert_eq!(
        vectors.calls().await,
        vec!["ensure_collection", "upsert", "mark_generation_committed"]
    );
    assert!(output.documents_prepared >= 1);
    assert!(output.chunks_prepared >= 1);
    assert!(output.vector_points_written >= 1);

    let points = vectors.points("axon-web-test").await;
    assert!(!points.is_empty());
    assert!(points.iter().all(|point| {
        point.payload["source_kind"].as_str() == Some("web")
            && point.payload["source_adapter"].as_str() == Some("web")
            && point.payload["source_scope"].as_str() == Some("page")
            && point.payload["web_domain"].as_str() == Some("example.com")
            && point.payload["visibility"].as_str() == Some("internal")
            && point.payload["redaction_status"].as_str() == Some("clean")
            && point.payload["committed_generation"]
                == committed_generation_payload(&output.generation)
            && point.payload["document_status"].as_str() == Some("published")
    }));
}

/// `embed = false` (source-pipeline.md Validation Checklist: "`embed=false`
/// never writes vectors") must produce zero embedding-provider calls and zero
/// vector-store upserts while still returning a `SourceResult`-shaped output
/// (a valid, non-error generation).
#[tokio::test]
async fn embed_false_writes_no_vectors_but_still_completes() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let mut no_embed_input = input();
    no_embed_input.embed = false;

    let output = index_web_source(no_embed_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(output.vector_points_written, 0);
    assert!(
        output.documents_prepared >= 1,
        "embed=false must still acquire, normalize, and prepare changed documents"
    );
    assert!(
        output.chunks_prepared >= 1,
        "embed=false must still chunk prepared documents"
    );
    assert_eq!(
        embedder.calls().await.len(),
        0,
        "embed=false must not call the embedding provider"
    );
    assert!(
        vectors.calls().await.is_empty(),
        "embed=false must not call the vector store"
    );
    assert!(vectors.points("axon-web-test").await.is_empty());
}

/// Page scope's `discover` is trivial identity-only (no content hash until
/// acquisition), so a second discover of the same URL always diffs as
/// "unchanged" against the first committed generation — the ledger short
/// circuits before `acquire` runs again.
#[tokio::test]
async fn unchanged_web_refresh_reuses_committed_generation_without_vector_work() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let first = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap();
    let embedding_calls = embedder.calls().await.len();
    let vector_calls = vectors.calls().await;

    let second = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(second.generation, first.generation);
    assert_eq!(second.documents_prepared, 0);
    assert_eq!(second.chunks_prepared, 0);
    assert_eq!(second.vector_points_written, 0);
    assert_eq!(embedder.calls().await.len(), embedding_calls);
    assert_eq!(vectors.calls().await, vector_calls);
}

#[tokio::test]
async fn map_scope_publishes_manifest_without_embedding_or_vectors() {
    let ledger = FakeLedgerStore::new();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");
    let providers = Arc::new(FakeAdapterProviders::new());
    let map_input = WebSourceIndexInput {
        source: "https://example.com/docs".to_string(),
        scope: SourceScope::Map,
        map_urls: vec![
            "https://example.com/docs/intro".to_string(),
            "https://example.com/docs/api".to_string(),
        ],
        crawl_options: MetadataMap::new(),
        output: OutputPolicy::default(),
        collection: "axon-web-test".to_string(),
        owner_id: "test-owner".to_string(),
        job_id: job_id(),
        auth_snapshot: None,
        embedding_provider_id: ProviderId::new("fake-embedding"),
        vector_provider_id: ProviderId::new("fake-vector"),
        embedding_model: "fake-embedding".to_string(),
        embedding_dimensions: 8,
        attempt: 1,
        embed: true,
        fetch_provider: providers.clone(),
        render_provider: providers,
        artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        document_cache: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
        event_store: None,
    };

    let output = index_web_source(map_input, &ledger, &embedder, &vectors)
        .await
        .unwrap();

    assert_eq!(
        ledger.committed_generation(&output.source_id).await,
        Some(output.generation.clone())
    );
    assert_eq!(output.documents_prepared, 0);
    assert_eq!(output.chunks_prepared, 0);
    assert_eq!(output.vector_points_written, 0);
    let generation = ledger
        .generation(&output.source_id, &output.generation)
        .await
        .unwrap();
    assert_eq!(generation.document_counts.discovered, 2);
    assert_eq!(generation.document_counts.prepared, 0);
    assert_eq!(generation.document_counts.embedded, 0);
    assert_eq!(generation.document_counts.published, 0);
    assert_eq!(embedder.calls().await.len(), 0);
    assert!(vectors.calls().await.is_empty());
}

#[tokio::test]
async fn publish_failure_rolls_back_web_vectors() {
    let ledger = FakeLedgerStore::new().with_publish_generation_failure();
    let embedder = FakeEmbeddingProvider::new("fake-embedding", 8);
    let vectors = FakeVectorStore::new("fake-vector");

    let err = index_web_source(input(), &ledger, &embedder, &vectors)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("publish_failed"),
        "unexpected error: {err:#}"
    );
    assert!(vectors.calls().await.contains(&"delete"));
    assert!(vectors.points("axon-web-test").await.is_empty());
}

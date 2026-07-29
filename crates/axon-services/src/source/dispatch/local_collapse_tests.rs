//! Differential regression coverage for finding C1 (pipeline-unification
//! review): local used to run its own private
//! leasing/diffing/generation/vectorize/publish pipeline
//! (`local_source/local_source_vectorize.rs`,
//! `local_source/local_source_publish.rs`) instead of the shared `non_web`
//! runner every other non-web family already used.
//!
//! These tests drive an identical local-filesystem fixture through BOTH:
//! - the still-live legacy pipeline (`local_source::index_local_source_with_job`
//!   — retained only for `query/code_search_refresh.rs`'s code-search
//!   auto-refresh caller, unchanged by the collapse), and
//! - the unified `source::dispatch::dispatch_local`, which now routes
//!   through `non_web::index_materialized_source` like git/feed/reddit/
//!   youtube/session/registry already did,
//!
//! and assert the two diverge on exactly the axes the review flagged, using
//! the legacy pipeline itself as the executable "pre-collapse" baseline —
//! no scratch copy needed, since that code is still compiled and reachable.

use std::sync::Arc;

use axon_adapters::local::LocalSourceAdapter;
use axon_api::source::{AuthSnapshot, JobId, JobPriority, SourceRequest};

use super::SourceExecutionContext;
use super::dispatch_local;
use crate::local_source::{LocalSourceIndexInput, LocalSourceSelectionPolicy};
use crate::test_support::source_context_with_local_sqlite_ledger;

fn test_execution() -> SourceExecutionContext {
    SourceExecutionContext::inline(SourceRequest::new("local-collapse-test"), None)
}

fn route_for(source: &str) -> axon_api::source::RoutePlan {
    crate::source::routing::resolve_source_route(&SourceRequest::new(source.to_string()))
        .expect("local test source should route")
        .route
}

fn local_auth_snapshot() -> AuthSnapshot {
    AuthSnapshot::trusted_cli("local-collapse-test")
}

fn legacy_input(
    root: std::path::PathBuf,
    collection: &str,
    runtime: &crate::context::TargetLocalSourceRuntime,
    embed: bool,
) -> LocalSourceIndexInput {
    LocalSourceIndexInput {
        root,
        collection: collection.to_string(),
        owner_id: "test-owner".to_string(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: None,
        embed,
        // Mirrors `code_search_refresh.rs` — the sole remaining legacy
        // caller never threads a routed `RoutePlan` through.
        route: None,
    }
}

/// Axis: "`ensure_collection` gated on `embed`" — `non_web`/every other
/// family gates `vector_store.ensure_collection` on `request.embed`; the
/// pre-collapse local pipeline
/// (`local_source/local_source_job.rs::index_local_source_with_progress`)
/// calls it unconditionally. Same one-file fixture, `embed: false`, through
/// both paths.
#[tokio::test]
async fn dispatch_local_embed_false_skips_ensure_collection_unlike_legacy_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("doc.md"), "# hello\n\nworld\n").unwrap();
    let root = dir.path().to_path_buf();
    let source = root.to_string_lossy().to_string();

    let legacy = source_context_with_local_sqlite_ledger()
        .await
        .expect("legacy harness");
    let legacy_runtime = legacy
        .ctx()
        .target_local_source_runtime()
        .expect("legacy target runtime");
    crate::local_source::index_local_source_with_job(
        legacy_input(
            root.clone(),
            "axon-legacy-embed-false",
            legacy_runtime,
            false,
        ),
        legacy_runtime.jobs.as_ref(),
        legacy_runtime.ledger.as_ref(),
        legacy_runtime.embedding_provider.as_ref(),
        legacy_runtime.vector_store.as_ref(),
    )
    .await
    .expect("legacy local index should succeed with embed=false");
    let legacy_calls = legacy.vectors().calls().await;
    assert!(
        legacy_calls.contains(&"ensure_collection"),
        "expected the still-live legacy local pipeline to call ensure_collection \
         even with embed=false (finding C1's ensure_collection-gating \
         divergence); got calls: {legacy_calls:?}"
    );

    let unified = source_context_with_local_sqlite_ledger()
        .await
        .expect("unified harness");
    let unified_runtime = unified
        .ctx()
        .target_local_source_runtime()
        .expect("unified target runtime");
    let snapshot = local_auth_snapshot();
    let route = route_for(&source);
    let cfg = axon_core::config::Config::default();
    dispatch_local(
        Arc::new(LocalSourceAdapter::new()),
        &cfg,
        unified_runtime,
        &source,
        "axon-unified-embed-false",
        "test-owner",
        Some(&snapshot),
        false,
        &route,
        &test_execution(),
    )
    .await
    .expect("unified local dispatch should succeed with embed=false");
    let unified_calls = unified.vectors().calls().await;
    assert!(
        !unified_calls.contains(&"ensure_collection"),
        "expected the unified dispatch_local (routed through non_web) to skip \
         ensure_collection when embed=false; got calls: {unified_calls:?}"
    );
}

/// Axes: "splits >512-chunk documents" and "embed batch priority". The
/// pre-collapse local pipeline (`local_source/local_source_vectorize.rs`)
/// batches changed *documents* by count only — no `split_oversized_document`
/// equivalent — and hardcodes `JobPriority::Background` on every embedding
/// batch. `non_web` (every other family, and now local) splits any document
/// whose chunk count exceeds the 512-chunk batch cap across multiple
/// embedding batches, and threads `execution.priority` through instead.
#[tokio::test]
async fn dispatch_local_streams_oversized_document_unlike_legacy_path() {
    let dir = tempfile::tempdir().unwrap();
    // Comfortably over the default markdown chunk target (500-2000 chars),
    // with enough distinct sections to blow well past the 512-chunk-per-batch
    // cap once chunked.
    let mut content = String::with_capacity(1_600_000);
    for i in 0..700 {
        content.push_str(&format!(
            "## Section {i}\n\nFiller paragraph {i} padded past the markdown \
             chunk target so each section chunks separately. {}\n\n",
            "lorem ipsum dolor sit amet consectetur adipiscing elit ".repeat(40),
        ));
    }
    std::fs::write(dir.path().join("big.md"), &content).unwrap();
    let root = dir.path().to_path_buf();
    let source = root.to_string_lossy().to_string();

    let legacy = source_context_with_local_sqlite_ledger()
        .await
        .expect("legacy harness");
    let legacy_runtime = legacy
        .ctx()
        .target_local_source_runtime()
        .expect("legacy target runtime");
    crate::local_source::index_local_source_with_job(
        legacy_input(root.clone(), "axon-legacy-oversized", legacy_runtime, true),
        legacy_runtime.jobs.as_ref(),
        legacy_runtime.ledger.as_ref(),
        legacy_runtime.embedding_provider.as_ref(),
        legacy_runtime.vector_store.as_ref(),
    )
    .await
    .expect("legacy local index should succeed");
    let legacy_calls = legacy.embedder().calls().await;
    let legacy_total_chunks: usize = legacy_calls.iter().map(|batch| batch.items.len()).sum();
    assert!(
        legacy_total_chunks > 512,
        "fixture should chunk past the 512-chunk batch cap; got {legacy_total_chunks} chunks \
         across {} batch(es)",
        legacy_calls.len()
    );
    assert_eq!(
        legacy_calls.len(),
        1,
        "expected the still-live legacy local pipeline to embed an oversized \
         document as a single unbounded batch (no split_oversized_document \
         equivalent); got {} batches with sizes {:?}",
        legacy_calls.len(),
        legacy_calls
            .iter()
            .map(|batch| batch.items.len())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        legacy_calls[0].priority,
        JobPriority::Background,
        "legacy local pipeline hardcodes JobPriority::Background on every embed batch"
    );

    let unified = source_context_with_local_sqlite_ledger()
        .await
        .expect("unified harness");
    let unified_runtime = unified
        .ctx()
        .target_local_source_runtime()
        .expect("unified target runtime");
    let snapshot = local_auth_snapshot();
    let route = route_for(&source);
    let cfg = axon_core::config::Config::default();
    dispatch_local(
        Arc::new(LocalSourceAdapter::new()),
        &cfg,
        unified_runtime,
        &source,
        "axon-unified-oversized",
        "test-owner",
        Some(&snapshot),
        true,
        &route,
        &test_execution(),
    )
    .await
    .expect("unified local dispatch should succeed");
    let unified_calls = unified.embedder().calls().await;
    let unified_total_chunks: usize = unified_calls.iter().map(|batch| batch.items.len()).sum();
    assert_eq!(
        unified_total_chunks, legacy_total_chunks,
        "both paths must embed the same total chunk count for the identical fixture \
         (only the batching shape may differ)"
    );
    assert!(
        unified_calls.len() > 1,
        "expected the unified dispatch_local (routed through non_web) to split an \
         oversized document across multiple embedding batches; got {} batch(es) with \
         sizes {:?}",
        unified_calls.len(),
        unified_calls
            .iter()
            .map(|batch| batch.items.len())
            .collect::<Vec<_>>()
    );
    assert!(
        unified_calls.iter().all(|batch| batch.items.len() <= 512),
        "every streamed batch must stay within the 512-chunk cap; got sizes: {:?}",
        unified_calls
            .iter()
            .map(|batch| batch.items.len())
            .collect::<Vec<_>>()
    );
    for batch in &unified_calls {
        assert_eq!(
            batch.priority,
            JobPriority::Normal,
            "unified dispatch_local must thread execution.priority (test default \
             JobPriority::Normal) instead of hardcoding Background"
        );
    }
}

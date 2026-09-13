//! Live RAG roundtrip integration test (TEST-H2).
//!
//! These tests are `#[ignore]`-gated because they require real Qdrant + TEI
//! services. The CI lane `live-rag-pr` runs them with:
//!
//! ```bash
//! cargo test --test rag_live_integration -- --ignored
//! ```
//!
//! Bring the services up locally with `just services-up`, then:
//!
//! ```bash
//! QDRANT_URL=http://127.0.0.1:53333 TEI_URL=http://127.0.0.1:52000 \
//!   cargo test --test rag_live_integration -- --ignored
//! ```
//!
//! The test embeds a small unique document into a throwaway collection, runs a
//! semantic query through the real `services::query` entry point, asserts the
//! embedded content comes back, and deletes the collection on the way out.

use std::sync::Arc;

use axon_api::source::SourceRequest;
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::query::query;
use axon_services::source::index_source;
use axon_services::types::Pagination;

/// Build a live config from env (`QDRANT_URL` / `TEI_URL`) targeting a unique
/// throwaway collection. Returns `None` when the required service URLs are not
/// set, so the test can skip cleanly rather than spuriously fail.
fn live_config(collection: &str, root: &std::path::Path) -> Option<Config> {
    let qdrant_url = std::env::var("QDRANT_URL").ok().filter(|s| !s.is_empty())?;
    let tei_url = std::env::var("TEI_URL").ok().filter(|s| !s.is_empty())?;
    let mut cfg = Config::default_minimal();
    cfg.qdrant_url = qdrant_url;
    cfg.tei_url = tei_url;
    cfg.collection = collection.to_string();
    cfg.embed = true;
    isolate_paths(&mut cfg, root);
    Some(cfg)
}

fn isolate_paths(cfg: &mut Config, root: &std::path::Path) {
    cfg.sqlite_path = root.join("jobs.db");
    cfg.output_dir = root.join("output");
    cfg.output_path = None;
    cfg.projection_output_dir = Some(root.join("projection"));
    cfg.source_local_allowed_roots = vec![root.to_path_buf()];
}

/// Delete the isolated collection and propagate provider errors.
async fn drop_collection(cfg: &Config) -> Result<(), reqwest::Error> {
    let endpoint = format!(
        "{}/collections/{}",
        cfg.qdrant_url.trim_end_matches('/'),
        cfg.collection
    );
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .delete(&endpoint)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[test]
#[ignore = "requires live Qdrant+TEI (just services-up)"]
fn embed_then_query_roundtrip_returns_embedded_content() {
    let root = tempfile::tempdir().expect("isolated live fixture");
    let collection = format!("axon_it_{}", uuid::Uuid::new_v4().simple());
    let Some(cfg) = live_config(&collection, root.path()) else {
        eprintln!("skipping: QDRANT_URL / TEI_URL not set");
        return;
    };

    // A distinctive sentence unlikely to collide with anything pre-indexed.
    let marker = format!("zorbax-{}", uuid::Uuid::new_v4().simple());
    let doc = format!(
        "The {marker} subsystem coordinates distributed widget reconciliation \
         across the homelab fleet. It batches reconciliation passes, applies \
         exponential backoff, and reports drift to the operator dashboard."
    );

    let tmp = root.path().join("input.md");
    std::fs::write(&tmp, &doc).expect("write temp doc");

    // The runtime is destroyed before the TempDir, including on assertion failure.
    let runtime = tokio::runtime::Runtime::new().expect("fixture runtime");
    let local = tokio::task::LocalSet::new();
    let outcome = runtime.block_on(local.run_until(async {
        let cleanup_cfg = cfg.clone();
        let outcome = tokio::task::spawn_local(async move {
            let ctx = ServiceContext::new_with_workers(Arc::new(cfg.clone()))
                .await
                .expect("build live service context");
            let mut request = SourceRequest::local_path(tmp.to_string_lossy(), false);
            request.collection = Some(collection.clone());
            let source_result = index_source(request, &ctx).await;

            let summary = source_result.expect("live source indexing must succeed");
            let docs_embedded = summary.counts.documents_total;
            assert!(
                docs_embedded >= 1,
                "expected at least one prepared document, got {:?}",
                summary.counts
            );
            assert!(
                summary.counts.vector_points_total >= 1,
                "expected at least one published vector point, got {:?}",
                summary.counts
            );

            // Query the real retrieval path for the unique marker.
            let res = query(
                &ctx,
                &cfg,
                &format!("{marker} widget reconciliation subsystem"),
                Pagination {
                    limit: 10,
                    offset: 0,
                },
            )
            .await
            .expect("live query must succeed");

            assert!(
                !res.results.is_empty(),
                "query must return at least one hit for the freshly embedded doc"
            );
            let found = res
                .results
                .iter()
                .any(|hit| hit.snippet.contains(&marker) || hit.snippet.contains("reconciliation"));
            assert!(
                found,
                "the embedded content must be retrievable; got hits: {:?}",
                res.results.iter().map(|h| &h.snippet).collect::<Vec<_>>()
            );
            ctx.shutdown_background_tasks().await;
        })
        .await;
        let cleanup = drop_collection(&cleanup_cfg).await;
        if let Err(error) = &cleanup {
            eprintln!("live collection cleanup failed: {error}");
        }
        (outcome, cleanup)
    }));
    drop(local);
    drop(runtime);
    let (outcome, cleanup) = outcome;
    if let Err(error) = outcome {
        if error.is_panic() {
            std::panic::resume_unwind(error.into_panic());
        }
        panic!("fixture task canceled: {error}");
    }
    cleanup.expect("delete live fixture collection");
}

#[test]
fn live_fixture_paths_are_isolated() {
    let root = tempfile::tempdir().unwrap();
    let mut cfg = Config::default_minimal();
    isolate_paths(&mut cfg, root.path());
    assert_eq!(cfg.sqlite_path, root.path().join("jobs.db"));
    assert_eq!(cfg.output_dir, root.path().join("output"));
    assert_eq!(
        cfg.projection_output_dir,
        Some(root.path().join("projection"))
    );
    assert!(cfg.output_path.is_none());
    assert_eq!(
        cfg.source_local_allowed_roots,
        vec![root.path().to_path_buf()]
    );
}

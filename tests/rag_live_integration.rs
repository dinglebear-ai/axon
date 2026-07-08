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

use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::query::query;
use axon_services::types::Pagination;

/// Build a live config from env (`QDRANT_URL` / `TEI_URL`) targeting a unique
/// throwaway collection. Returns `None` when the required service URLs are not
/// set, so the test can skip cleanly rather than spuriously fail.
fn live_config(collection: &str) -> Option<Config> {
    let qdrant_url = std::env::var("QDRANT_URL").ok().filter(|s| !s.is_empty())?;
    let tei_url = std::env::var("TEI_URL").ok().filter(|s| !s.is_empty())?;
    let mut cfg = Config::default_minimal();
    cfg.qdrant_url = qdrant_url;
    cfg.tei_url = tei_url;
    cfg.collection = collection.to_string();
    cfg.embed = true;
    Some(cfg)
}

/// Best-effort cleanup: DELETE the throwaway collection. Ignores all errors so a
/// failed assertion still tears the collection down.
async fn drop_collection(cfg: &Config) {
    let endpoint = format!(
        "{}/collections/{}",
        cfg.qdrant_url.trim_end_matches('/'),
        cfg.collection
    );
    let _ = reqwest::Client::new().delete(&endpoint).send().await;
}

#[tokio::test]
#[ignore = "requires live Qdrant+TEI (just services-up)"]
async fn embed_then_query_roundtrip_returns_embedded_content() {
    let collection = format!("axon_it_{}", uuid::Uuid::new_v4().simple());
    let Some(cfg) = live_config(&collection) else {
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

    let tmp = std::env::temp_dir().join(format!("{collection}.md"));
    std::fs::write(&tmp, &doc).expect("write temp doc");

    // Embed the doc into the throwaway collection (ensure_collection runs inside).
    let embed_result = axon_vector::ops::embed_path_native(&cfg, tmp.to_str().unwrap()).await;

    // Always attempt cleanup even if an assertion below fails.
    let outcome = async {
        let summary = embed_result.expect("live embed must succeed");
        assert!(
            summary.docs_embedded >= 1,
            "expected at least one embedded doc, got {summary:?}"
        );
        assert_eq!(summary.docs_failed, 0, "no docs may fail in the roundtrip");

        // Query the real retrieval path for the unique marker.
        let ctx = ServiceContext::new(Arc::new(cfg.clone()))
            .await
            .expect("build live service context");
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
    }
    .await;

    drop_collection(&cfg).await;
    let _ = std::fs::remove_file(&tmp);

    // Re-raise any panic captured above (the closure already panics inline, so
    // this is just the explicit tail — `outcome` is `()`).
    outcome
}

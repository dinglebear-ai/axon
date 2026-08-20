use std::sync::Arc;
use std::time::Duration;

use axon_core::config::Config;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use httpmock::MockServer;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Barrier;

use super::{
    artifact_candidate_sink_from_values, invalidate_embedding_identity_cache,
    resolve_embedding_identity, tei_max_attempts,
};
use crate::context::TargetLocalSourceRuntime;

#[tokio::test]
async fn artifact_candidate_sink_configuration_is_all_or_nothing() {
    let disabled = artifact_candidate_sink_from_values(None, None).expect("disabled sink");
    assert_eq!(disabled.capabilities().await.unwrap().name, "noop");

    let empty = artifact_candidate_sink_from_values(Some(String::new()), Some(String::new()))
        .expect("empty values disable the optional sink");
    assert_eq!(empty.capabilities().await.unwrap().name, "noop");

    assert!(
        artifact_candidate_sink_from_values(Some("https://depot.example".to_string()), None,)
            .is_err()
    );
    assert!(artifact_candidate_sink_from_values(None, Some("write-token".to_string())).is_err());
    assert!(
        artifact_candidate_sink_from_values(
            Some("not a URL".to_string()),
            Some("write-token".to_string()),
        )
        .is_err()
    );

    let configured = artifact_candidate_sink_from_values(
        Some("https://depot.example".to_string()),
        Some("write-token".to_string()),
    )
    .expect("valid Depot sink");
    assert_eq!(configured.capabilities().await.unwrap().name, "depot-http");
}

/// `tei_max_attempts` is the one place `cfg.tei_max_retries` becomes the real
/// attempt budget threaded into `TeiEmbeddingConfig::max_attempts` — previously
/// `TeiEmbeddingProvider` always used a hardcoded `MAX_ATTEMPTS = 6` regardless
/// of `[providers.embedding].max-retries`/`TEI_MAX_RETRIES`.
#[test]
fn tei_max_attempts_reflects_configured_retry_count_not_a_hardcoded_default() {
    let mut cfg = Config::test_default();

    cfg.tei_max_retries = 5;
    assert_eq!(
        tei_max_attempts(&cfg),
        6,
        "default tei_max_retries=5 should still yield the historical 6 total attempts"
    );

    cfg.tei_max_retries = 2;
    assert_eq!(
        tei_max_attempts(&cfg),
        3,
        "a non-default tei_max_retries must change the computed attempt budget"
    );

    cfg.tei_max_retries = 0;
    assert_eq!(
        tei_max_attempts(&cfg),
        1,
        "zero retries still allows exactly one (the initial) attempt"
    );
}

/// `from_config` builds the three real stores + reservations from the shared
/// SQLite pool (one runtime DB) and a dummy Qdrant URL. The ledger binds to the
/// caller-supplied pool via `from_pool` without running its own migrations (the
/// tables are owned by the shared migration runner), and the Qdrant constructor
/// does not connect.
///
/// The embedding identity is now derived from the live TEI `/info` + a probe
/// embed. To keep this unit test hermetic and deterministic, `tei_url` points at
/// a closed loopback port so the derivation always fails fast and falls back to
/// the configured defaults — proving the fallback path stamps the model/dims.
#[tokio::test]
async fn from_config_falls_back_to_default_embedding_identity_when_tei_unreachable() {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = "http://127.0.0.1:53333".to_string();
    // Closed port → derivation fails fast → fallback identity.
    cfg.tei_url = "http://127.0.0.1:1".to_string();
    cfg.tei_request_timeout_ms = 250;
    cfg.embed_prep_concurrency = 3;
    cfg.embed_pool_max_inputs = 640;

    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());
    // The ledger binds to this shared pool (no separate ledger.db, no eager I/O).
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool");
    axon_jobs::migrations::apply_all_migrations(&pool)
        .await
        .expect("shared runtime migrations");

    let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool)
        .await
        .expect("build target local-source runtime");

    assert_eq!(runtime.embedding_provider_id.0, "target-local-embed");
    assert_eq!(runtime.vector_provider_id.0, "target-local-vector");
    // Fallback identity when the live provider cannot be reached.
    assert_eq!(runtime.embedding_model, "Qwen3-Embedding-0.6B");
    assert_eq!(runtime.embedding_dimensions, 1024);
    assert_eq!(runtime.document_prepare_concurrency, 3);
    assert_eq!(runtime.embed_pool_max_inputs, 640);
}

#[tokio::test]
async fn embedding_identity_cache_singleflights_cold_probes_and_can_be_invalidated() {
    let server = MockServer::start_async().await;
    let info = server
        .mock_async(|when, then| {
            when.method("GET").path("/info");
            then.status(200)
                .delay(Duration::from_millis(100))
                .json_body(serde_json::json!({ "model_id": "acme/embedding" }));
        })
        .await;
    let embed = server
        .mock_async(|when, then| {
            when.method("POST").path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
        })
        .await;
    let mut cfg = Config::test_default();
    cfg.tei_url = server.base_url();
    cfg.tei_request_timeout_ms = 1_000;
    invalidate_embedding_identity_cache(&cfg);

    let callers = 8;
    let barrier = Arc::new(Barrier::new(callers + 1));
    let mut tasks = Vec::with_capacity(callers);
    for _ in 0..callers {
        let barrier = Arc::clone(&barrier);
        let cfg = cfg.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            resolve_embedding_identity(&cfg).await
        }));
    }
    barrier.wait().await;
    for task in tasks {
        let identity = task.await.expect("identity task");
        assert_eq!(identity.model, "acme/embedding");
        assert_eq!(identity.dimensions, 3);
    }
    info.assert_calls_async(1).await;
    embed.assert_calls_async(1).await;

    // A warm read is cache-only. Explicit invalidation makes the next caller
    // reprobe immediately instead of waiting for the 30-second positive TTL.
    let warm = resolve_embedding_identity(&cfg).await;
    assert_eq!(warm.dimensions, 3);
    info.assert_calls_async(1).await;
    invalidate_embedding_identity_cache(&cfg);
    let refreshed = resolve_embedding_identity(&cfg).await;
    assert_eq!(refreshed.dimensions, 3);
    info.assert_calls_async(2).await;
    embed.assert_calls_async(2).await;
}

#[test]
fn scheduler_authority_canonicalizes_equivalent_sqlite_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("nested");
    std::fs::create_dir_all(&nested).expect("nested dir");
    let database = dir.path().join("jobs.db");
    std::fs::write(&database, []).expect("database placeholder");
    let equivalent = nested.join("..").join("jobs.db");

    assert_eq!(
        super::scheduler_authority_id(&database),
        super::scheduler_authority_id(&equivalent)
    );
}

use std::sync::Arc;

use axon_core::config::Config;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use sqlx::sqlite::SqlitePoolOptions;

use crate::context::TargetLocalSourceRuntime;

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

    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());
    // The ledger binds to this shared pool (no separate ledger.db, no eager I/O).
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite pool");

    let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs, pool)
        .await
        .expect("build target local-source runtime");

    assert_eq!(runtime.embedding_provider_id.0, "target-local-embed");
    assert_eq!(runtime.vector_provider_id.0, "target-local-vector");
    // Fallback identity when the live provider cannot be reached.
    assert_eq!(runtime.embedding_model, "Qwen3-Embedding-0.6B");
    assert_eq!(runtime.embedding_dimensions, 1024);
}

use std::sync::Arc;

use axon_core::config::Config;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use sqlx::sqlite::SqlitePoolOptions;

use crate::context::TargetLocalSourceRuntime;

/// `from_config` builds the three real stores + reservations from the shared
/// SQLite pool (one runtime DB) and dummy Qdrant/TEI URLs. No live Qdrant/TEI is
/// required: the vector/embedding constructors do not connect, and the ledger
/// binds to the caller-supplied pool via `from_pool` without running its own
/// migrations (the tables are owned by the shared migration runner).
#[tokio::test]
async fn from_config_populates_provider_metadata() {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = "http://127.0.0.1:53333".to_string();
    cfg.tei_url = "http://127.0.0.1:52000".to_string();

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
    assert_eq!(runtime.embedding_model, "Qwen3-Embedding-0.6B");
    assert_eq!(runtime.embedding_dimensions, 1024);
}

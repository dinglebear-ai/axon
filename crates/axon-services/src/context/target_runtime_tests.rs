use std::sync::Arc;

use axon_core::config::Config;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};

use crate::context::TargetLocalSourceRuntime;

/// `from_config` builds the three real stores + reservations against a temp
/// ledger DB and dummy Qdrant/TEI URLs. No live Qdrant/TEI is required: the
/// vector/embedding constructors do not connect, and `SqliteLedgerStore::connect`
/// only runs migrations against a real (temp) SQLite file.
#[tokio::test]
async fn from_config_populates_provider_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let mut cfg = Config::test_default();
    // sqlite_path's parent dir is where the sibling ledger.db is created.
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.qdrant_url = "http://127.0.0.1:53333".to_string();
    cfg.tei_url = "http://127.0.0.1:52000".to_string();

    let jobs: Arc<dyn JobStore> = Arc::new(FakeJobWatchStore::new());

    let runtime = TargetLocalSourceRuntime::from_config(&cfg, jobs)
        .await
        .expect("build target local-source runtime");

    assert_eq!(runtime.embedding_provider_id.0, "target-local-embed");
    assert_eq!(runtime.vector_provider_id.0, "target-local-vector");
    assert_eq!(runtime.embedding_model, "Qwen3-Embedding-0.6B");
    assert_eq!(runtime.embedding_dimensions, 1024);

    // The sibling ledger DB was created next to the jobs DB.
    assert!(tmp.path().join("ledger.db").exists());
}

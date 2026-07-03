//! Test helpers for `axon-graph` consumers.
//!
//! Provides quick constructors for in-memory graph stores so higher layers can
//! exercise graph behavior without standing up external services.

use crate::sqlite::SqliteGraphStore;
use crate::store::FakeGraphStore;

/// An in-memory SQLite-backed graph store with the schema applied.
pub async fn in_memory_sqlite_store() -> SqliteGraphStore {
    SqliteGraphStore::connect(":memory:")
        .await
        .expect("in-memory graph store must open")
}

/// A fresh in-memory fake graph store.
pub fn fake_store() -> FakeGraphStore {
    FakeGraphStore::new()
}

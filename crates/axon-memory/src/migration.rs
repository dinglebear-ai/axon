//! In-crate SQLite schema for the durable memory store.
//!
//! Four tables back the store, matching the memory-contract data model:
//! - `memory_records`   — one row per memory (type/status/body/scope/decay/history)
//! - `memory_links`     — evidence-backed links from a memory to sources/entities
//! - `memory_reinforcement` — append-only positive-use signals (reinforce log)
//! - `memory_reviews`   — review-queue entries with reason + timestamps
//!
//! `ensure_schema` is idempotent (`CREATE TABLE IF NOT EXISTS`) so it is safe to
//! call on every store construction.

use axon_api::migration::{MigrationSet, SqlMigration};
use rusqlite::Connection;

/// Schema version stamped into `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// Namespace under which the composed cross-crate runner tracks memory
/// migrations.
pub const MIGRATION_NAMESPACE: &str = "memory";

/// The durable memory schema. Single source of truth shared by the rusqlite
/// standalone path ([`ensure_schema`]) and the composed cross-crate SQLite
/// runner in `axon-jobs` (via [`migration_set`]).
const SCHEMA_SQL: &str = include_str!("migrations/0001_create_memory_tables.sql");

/// Ordered memory migration set for the composed cross-crate runner.
///
/// `SqliteMemoryStore::open`/`in_memory` still call [`ensure_schema`] against
/// their own rusqlite connection, but the production runtime shares one SQLite
/// pool and gets these tables from the unified runner over sqlx.
pub const MIGRATIONS: &[SqlMigration] = &[SqlMigration {
    version: 1,
    name: "0001_create_memory_tables",
    sql: SCHEMA_SQL,
}];

/// The memory [`MigrationSet`] for composition into the unified runner.
pub fn migration_set() -> MigrationSet {
    MigrationSet::new(MIGRATION_NAMESPACE, MIGRATIONS)
}

/// Create all memory tables and indices if they do not exist, enable foreign
/// keys, and stamp the schema version.
pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;

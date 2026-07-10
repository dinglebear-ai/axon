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
pub const SCHEMA_VERSION: i64 = 3;

/// Namespace under which the composed cross-crate runner tracks memory
/// migrations.
pub const MIGRATION_NAMESPACE: &str = "memory";

/// The durable memory schema. Single source of truth shared by the rusqlite
/// standalone path ([`ensure_schema`]) and the composed cross-crate SQLite
/// runner in `axon-jobs` (via [`migration_set`]).
const SCHEMA_SQL: &str = include_str!("migrations/0001_create_memory_tables.sql");

/// Composite indexes for batch recall/review/import query patterns
/// (0001's single-column indexes don't help a compound WHERE/ORDER BY as
/// well as a matching composite index).
const BATCH_INDEXES_SQL: &str = include_str!("migrations/0002_batch_recovery_indexes.sql");

/// Adds the `visibility` security-classification column (contract "Security
/// and Redaction": "classify every memory by visibility").
const VISIBILITY_COLUMN_SQL: &str = include_str!("migrations/0003_add_memory_visibility.sql");

/// Ordered memory migration set for the composed cross-crate runner.
///
/// `SqliteMemoryStore::open`/`in_memory` still call [`ensure_schema`] against
/// their own rusqlite connection, but the production runtime shares one SQLite
/// pool and gets these tables from the unified runner over sqlx.
pub const MIGRATIONS: &[SqlMigration] = &[
    SqlMigration {
        version: 1,
        name: "0001_create_memory_tables",
        sql: SCHEMA_SQL,
    },
    SqlMigration {
        version: 2,
        name: "0002_batch_recovery_indexes",
        sql: BATCH_INDEXES_SQL,
    },
    SqlMigration {
        version: 3,
        name: "0003_add_memory_visibility",
        sql: VISIBILITY_COLUMN_SQL,
    },
];

/// The memory [`MigrationSet`] for composition into the unified runner.
pub fn migration_set() -> MigrationSet {
    MigrationSet::new(MIGRATION_NAMESPACE, MIGRATIONS)
}

/// Create all memory tables and indices if they do not exist, enable foreign
/// keys, and stamp the schema version.
pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.execute_batch(BATCH_INDEXES_SQL)?;
    ensure_visibility_column(conn)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

/// `ALTER TABLE ... ADD COLUMN` (unlike `CREATE TABLE`/`CREATE INDEX IF NOT
/// EXISTS`) is not idempotent, so — unlike [`SCHEMA_SQL`]/[`BATCH_INDEXES_SQL`]
/// — it cannot be re-run unconditionally on every `ensure_schema` call. Guard
/// it with a `pragma_table_info` existence check instead.
fn ensure_visibility_column(conn: &Connection) -> rusqlite::Result<()> {
    let has_column = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('memory_records') WHERE name = 'visibility'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if !has_column {
        conn.execute_batch(VISIBILITY_COLUMN_SQL)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;

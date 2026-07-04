//! Observe migration set, exposed for the composed cross-crate runner.
//!
//! `SqliteObservabilitySink::from_pool` still runs `sqlx::migrate!` for the
//! crate's standalone/in-memory path, but the production runtime shares ONE
//! SQLite pool with the jobs runtime and every other domain store. The composed
//! runner in `axon-jobs` applies this set into that shared pool via
//! [`axon_api::migration`], so the observability tables exist on the unified DB
//! without observe running its own colliding migrator.

use axon_api::migration::{MigrationSet, SqlMigration};

/// Namespace under which the composed cross-crate runner tracks observe
/// migrations.
pub const MIGRATION_NAMESPACE: &str = "observe";

/// Ordered observe migration set (durable observability tables).
pub const MIGRATIONS: &[SqlMigration] = &[SqlMigration {
    version: 1,
    name: "0001_create_observability_tables",
    sql: include_str!("migrations/0001_create_observability_tables.sql"),
}];

/// The observe [`MigrationSet`] for composition into the unified runner.
pub fn migration_set() -> MigrationSet {
    MigrationSet::new(MIGRATION_NAMESPACE, MIGRATIONS)
}

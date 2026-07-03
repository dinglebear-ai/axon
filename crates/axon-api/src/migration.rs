//! Transport-neutral migration descriptors shared by every SQLite-backed crate.
//!
//! The runtime uses ONE unified SQLite pool for the jobs runtime and every
//! domain store (ledger, observe, graph, memory). Historically each crate ran
//! its own `sqlx::migrate!` against that pool, which collides on the shared
//! `_sqlx_migrations` table (each set restarts numbering at `0001`), and
//! `axon-jobs` migration `0017` hand-copied the ledger's contract tables to
//! paper over the missing runner. This module defines the small vocabulary the
//! composed cross-crate runner (`axon_jobs::migrations`) uses instead: each
//! crate exposes an ordered [`MigrationSet`] of raw SQL, and the runner applies
//! every set in dependency order against the same pool, tracking applied
//! versions per-namespace so numbering spaces never collide.
//!
//! Descriptors carry only `&'static str` SQL embedded via `include_str!`; they
//! hold no pool/connection type so this crate stays transport-neutral and every
//! store crate (which already depends on `axon-api`) can expose one without a
//! new dependency.

/// A single ordered schema migration: a version, a human-readable name, and the
/// raw SQL to execute. SQL is expected to be idempotent (`CREATE TABLE IF NOT
/// EXISTS` / `CREATE INDEX IF NOT EXISTS`) so re-running against an already
/// migrated store is a no-op even before the applied-version guard.
#[derive(Debug, Clone, Copy)]
pub struct SqlMigration {
    /// Monotonic version within the owning [`MigrationSet::namespace`]. Versions
    /// are unique and dense (`1, 2, 3, …`) per namespace, matching the
    /// zero-padded filename prefixes on disk.
    pub version: i64,
    /// Migration name (typically the SQL filename without extension), surfaced
    /// in error messages so a failed migration is reported with its id.
    pub name: &'static str,
    /// Raw SQL body. May contain multiple statements separated by `;`.
    pub sql: &'static str,
}

/// One crate's ordered migration set, tagged with a stable `namespace`.
///
/// The composed runner records applied `(namespace, version)` pairs in a single
/// `axon_applied_migrations` table, so two crates that both number their first
/// migration `0001` never collide.
#[derive(Debug, Clone, Copy)]
pub struct MigrationSet {
    /// Stable namespace identifier for this crate's migrations (e.g. `"jobs"`,
    /// `"ledger"`). Must be unique across every set the runner composes.
    pub namespace: &'static str,
    /// Migrations in apply order. The runner asserts strictly increasing,
    /// gap-free versions starting at `1`.
    pub migrations: &'static [SqlMigration],
}

impl MigrationSet {
    /// Construct a set from its namespace and slice of migrations.
    pub const fn new(namespace: &'static str, migrations: &'static [SqlMigration]) -> Self {
        Self {
            namespace,
            migrations,
        }
    }
}

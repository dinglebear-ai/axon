//! Composed cross-crate SQLite migration runner.
//!
//! The runtime uses ONE unified SQLite pool for the jobs runtime AND every
//! domain store (ledger, observe, graph, memory) — see
//! `docs/pipeline-unification/runtime/storage-contract.md`. Before this runner
//! existed, each crate ran its own migration set against that pool. The
//! composed runner gives every crate an independent namespace and preserves
//! dependency order.
//!
//! [`apply_all_migrations`] applies each crate's [`MigrationSet`] against the
//! same pool in dependency order and records applied `(namespace, version)`
//! pairs in one `axon_applied_migrations` table, so numbering spaces never
//! collide and re-running is a no-op.
//!
//! ## Order (dependency-first)
//!
//! 1. **ledger** — SOLE creator of the seven contract tables
//!    (`sources`, `source_generations`, `source_manifests`, `source_items`,
//!    `document_status`, `cleanup_debt`, `leases`). Runs FIRST so `jobs.source_id`
//!    can FK to `sources(source_id)` in the same file.
//! 2. **jobs** — the canonical job runtime tables (`jobs`, `job_events`, and
//!    source watches). `jobs` FKs `sources`, which ledger created above.
//! 3. **observe**, **graph**, **memory** — orphan domain stores; independent of
//!    each other, applied after the write-plane tables exist.

use axon_api::migration::{MigrationSet, SqlMigration};
use sqlx::{Executor, SqlitePool};

#[path = "migrations/identity.rs"]
mod identity;

/// Ordered jobs migration set, exposed for the composed runner.
///
/// The clean-break jobs schema is a single canonical baseline. Older stores
/// are rejected by the cutover audit and must be reset instead of migrated.
pub const JOBS_MIGRATIONS: &[SqlMigration] = &[SqlMigration {
    version: 1,
    name: "0001_canonical_jobs",
    sql: include_str!("migrations/0001_canonical_jobs.sql"),
}];

/// Namespace under which the composed runner tracks jobs migrations.
pub const JOBS_NAMESPACE: &str = "jobs";

/// The jobs [`MigrationSet`] for composition into the unified runner.
pub fn jobs_migration_set() -> MigrationSet {
    MigrationSet::new(JOBS_NAMESPACE, JOBS_MIGRATIONS)
}

/// The migration sets to compose, in dependency order. `ledger` FIRST so its
/// `sources` table exists before `jobs` FKs it; the orphan domain stores follow.
fn composed_sets() -> [MigrationSet; 5] {
    [
        axon_ledger::migration::migration_set(),
        jobs_migration_set(),
        axon_observe::migration::migration_set(),
        axon_graph::migration::migration_set(),
        axon_memory::migration::migration_set(),
    ]
}

/// Apply every crate's migration set against the shared pool, in dependency
/// order, atomically.
///
/// A fresh database receives the complete canonical schema and exact migration
/// receipts in one transaction. An existing database is accepted only when its
/// epoch, receipt names/checksums, tables, and foreign keys match exactly.
///
/// A failure is reported with the offending `namespace/name` id, satisfying the
/// schema contract's "migration failure is reported with migration id" rule.
pub async fn apply_all_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let sets = composed_sets();
    validate_sets(&sets)?;

    let mut tx = pool.begin().await?;
    let fresh = identity::validate_before_mutation(&mut tx, &sets).await?;
    if fresh {
        ensure_applied_table(&mut tx).await?;
        for set in sets {
            apply_set(&mut tx, set).await?;
        }
        identity::stamp_schema_epoch(&mut tx).await?;
        identity::validate_canonical(&mut tx, &sets).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Create the single applied-migrations ledger table if absent.
async fn ensure_applied_table(connection: &mut sqlx::SqliteConnection) -> Result<(), sqlx::Error> {
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS axon_applied_migrations (
            namespace  TEXT NOT NULL,
            version    INTEGER NOT NULL,
            name       TEXT NOT NULL,
            checksum   TEXT NOT NULL,
            schema_epoch INTEGER NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (namespace, version)
        )",
        )
        .await?;
    Ok(())
}

/// Apply one namespace's migrations in version order.
async fn apply_set(
    connection: &mut sqlx::SqliteConnection,
    set: MigrationSet,
) -> Result<(), sqlx::Error> {
    for &migration in set.migrations {
        run_migration(connection, set.namespace, migration).await?;
        record_applied(connection, set.namespace, migration).await?;
    }
    Ok(())
}

/// Run a single migration's SQL. Multi-statement bodies are executed via the
/// connection's batch executor. A failure surfaces the migration id so
/// operators can locate the offending file.
async fn run_migration(
    connection: &mut sqlx::SqliteConnection,
    namespace: &'static str,
    migration: SqlMigration,
) -> Result<(), sqlx::Error> {
    connection.execute(migration.sql).await.map_err(|e| {
        sqlx::Error::Configuration(
            format!(
                "migration {namespace}/{name} (v{version}) failed: {e}",
                name = migration.name,
                version = migration.version,
            )
            .into(),
        )
    })?;
    Ok(())
}

/// Record the exact identity of an applied migration.
async fn record_applied(
    connection: &mut sqlx::SqliteConnection,
    namespace: &'static str,
    migration: SqlMigration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO axon_applied_migrations \
         (namespace, version, name, checksum, schema_epoch) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(namespace)
    .bind(migration.version)
    .bind(migration.name)
    .bind(identity::migration_checksum(migration.sql))
    .bind(identity::SCHEMA_EPOCH)
    .execute(connection)
    .await?;
    Ok(())
}

/// Assert the set's versions are contiguous and strictly increasing.
fn validate_versions(set: &MigrationSet) -> Result<(), sqlx::Error> {
    let Some(first) = set.migrations.first() else {
        return Ok(());
    };
    for (index, migration) in set.migrations.iter().enumerate() {
        let expected = first.version + index as i64;
        if migration.version != expected {
            return Err(sqlx::Error::Configuration(
                format!(
                    "migration set '{ns}' out of order: expected version {expected} at position \
                     {index}, found {found} ({name})",
                    ns = set.namespace,
                    found = migration.version,
                    name = migration.name,
                )
                .into(),
            ));
        }
    }
    Ok(())
}

fn validate_sets(sets: &[MigrationSet]) -> Result<(), sqlx::Error> {
    let mut namespaces = std::collections::BTreeSet::new();
    for set in sets {
        if !namespaces.insert(set.namespace) {
            return Err(sqlx::Error::Configuration(
                format!("duplicate migration namespace '{}'", set.namespace).into(),
            ));
        }
        validate_versions(set)?;
        if set
            .migrations
            .first()
            .is_some_and(|migration| migration.version != 1)
        {
            return Err(sqlx::Error::Configuration(
                format!("migration set '{}' must begin at version 1", set.namespace).into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;

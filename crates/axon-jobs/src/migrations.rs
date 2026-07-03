//! Composed cross-crate SQLite migration runner.
//!
//! The runtime uses ONE unified SQLite pool for the jobs runtime AND every
//! domain store (ledger, observe, graph, memory) — see
//! `docs/pipeline-unification/runtime/storage-contract.md`. Before this runner
//! existed, each crate ran its own `sqlx::migrate!` against that pool, which
//! collides on the shared `_sqlx_migrations` table (every set restarts
//! numbering at `0001`), and `axon-jobs` migration `0017` hand-copied the
//! ledger's seven contract tables to paper over the missing runner — the ledger
//! split-brain this module eliminates.
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
//! 2. **jobs** — the job runtime tables (`jobs`, `job_events`, watch/freshness,
//!    plus the legacy `axon_source_*` tables that still back `axon-source-ledger`
//!    during the cutover). `jobs` FKs `sources`, which ledger created above.
//! 3. **observe**, **graph**, **memory** — orphan domain stores; independent of
//!    each other, applied after the write-plane tables exist.

use axon_api::migration::{MigrationSet, SqlMigration};
use sqlx::{Executor, SqlitePool};

/// Ordered jobs migration set, exposed for the composed runner.
///
/// Mirrors the `.sql` files under `src/migrations/` one-for-one; the runner
/// applies them under the `jobs` namespace. `0017` no longer creates the ledger
/// contract tables — the ledger set above owns them.
pub const JOBS_MIGRATIONS: &[SqlMigration] = &[
    SqlMigration {
        version: 1,
        name: "0001_create_tables",
        sql: include_str!("migrations/0001_create_tables.sql"),
    },
    SqlMigration {
        version: 2,
        name: "0002_create_watch_tables",
        sql: include_str!("migrations/0002_create_watch_tables.sql"),
    },
    SqlMigration {
        version: 3,
        name: "0003_add_status_checks",
        sql: include_str!("migrations/0003_add_status_checks.sql"),
    },
    SqlMigration {
        version: 4,
        name: "0004_status_created_at_index",
        sql: include_str!("migrations/0004_status_created_at_index.sql"),
    },
    SqlMigration {
        version: 5,
        name: "0005_add_attempt_metadata",
        sql: include_str!("migrations/0005_add_attempt_metadata.sql"),
    },
    SqlMigration {
        version: 6,
        name: "0006_create_ingest_payloads",
        sql: include_str!("migrations/0006_create_ingest_payloads.sql"),
    },
    SqlMigration {
        version: 7,
        name: "0007_create_watch_url_state",
        sql: include_str!("migrations/0007_create_watch_url_state.sql"),
    },
    SqlMigration {
        version: 8,
        name: "0008_add_embed_fs_namespace",
        sql: include_str!("migrations/0008_add_embed_fs_namespace.sql"),
    },
    SqlMigration {
        version: 9,
        name: "0009_create_memory_tables",
        sql: include_str!("migrations/0009_create_memory_tables.sql"),
    },
    SqlMigration {
        version: 10,
        name: "0010_create_session_watch_tables",
        sql: include_str!("migrations/0010_create_session_watch_tables.sql"),
    },
    SqlMigration {
        version: 11,
        name: "0011_add_session_watch_checkpoint_state",
        sql: include_str!("migrations/0011_add_session_watch_checkpoint_state.sql"),
    },
    SqlMigration {
        version: 12,
        name: "0012_add_memory_runtime_metadata",
        sql: include_str!("migrations/0012_add_memory_runtime_metadata.sql"),
    },
    SqlMigration {
        version: 13,
        name: "0013_add_job_progress_json",
        sql: include_str!("migrations/0013_add_job_progress_json.sql"),
    },
    SqlMigration {
        version: 14,
        name: "0014_backfill_active_job_progress_json",
        sql: include_str!("migrations/0014_backfill_active_job_progress_json.sql"),
    },
    SqlMigration {
        version: 15,
        name: "0015_freshness",
        sql: include_str!("migrations/0015_freshness.sql"),
    },
    SqlMigration {
        version: 16,
        name: "0016_freshness_run_retention_index",
        sql: include_str!("migrations/0016_freshness_run_retention_index.sql"),
    },
    SqlMigration {
        version: 17,
        name: "0017_source_ledger",
        sql: include_str!("migrations/0017_source_ledger.sql"),
    },
    SqlMigration {
        version: 18,
        name: "0018_unified_jobs_observability",
        sql: include_str!("migrations/0018_unified_jobs_observability.sql"),
    },
];

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
/// order, idempotently.
///
/// Records applied `(namespace, version)` pairs in `axon_applied_migrations` so
/// a fresh DB migrates cleanly and repeated runs are no-ops. Migration SQL is
/// itself idempotent (`CREATE ... IF NOT EXISTS`); the applied-version guard
/// additionally skips already-applied migrations without re-executing them.
///
/// A failure is reported with the offending `namespace/name` id, satisfying the
/// schema contract's "migration failure is reported with migration id" rule.
pub async fn apply_all_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    ensure_applied_table(pool).await?;
    // Adopt a pre-cutover jobs DB that was migrated by the old per-crate
    // `sqlx::migrate!` runner: seed already-applied jobs versions from
    // `_sqlx_migrations` so the composed runner does not re-run destructive
    // `ALTER TABLE` migrations that lack `IF NOT EXISTS` semantics. A fresh DB
    // has no `_sqlx_migrations` table and this is a no-op. (Contract:
    // "empty-store bootstrap and upgraded-store migration both use the same
    // migration runner".)
    backfill_legacy_sqlx_jobs(pool).await?;
    for set in composed_sets() {
        apply_set(pool, &set).await?;
    }
    Ok(())
}

/// Seed the `jobs` namespace of `axon_applied_migrations` from a legacy
/// `_sqlx_migrations` table, if one exists. Only successful versions that this
/// binary knows about are adopted; unknown/newer or failed versions are left
/// for the normal apply loop to handle (or error) as usual.
async fn backfill_legacy_sqlx_jobs(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let has_legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if has_legacy == 0 {
        return Ok(());
    }
    for migration in JOBS_MIGRATIONS {
        let applied_ok: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ? AND success = 1",
        )
        .bind(migration.version)
        .fetch_one(pool)
        .await?;
        if applied_ok > 0 {
            record_applied(pool, JOBS_NAMESPACE, migration).await?;
        }
    }
    Ok(())
}

/// Create the single applied-migrations ledger table if absent.
async fn ensure_applied_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    pool.execute(
        "CREATE TABLE IF NOT EXISTS axon_applied_migrations (
            namespace  TEXT NOT NULL,
            version    INTEGER NOT NULL,
            name       TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (namespace, version)
        )",
    )
    .await?;
    Ok(())
}

/// Apply one namespace's migrations in version order, skipping any already
/// recorded in `axon_applied_migrations`.
async fn apply_set(pool: &SqlitePool, set: &MigrationSet) -> Result<(), sqlx::Error> {
    validate_versions(set)?;
    for migration in set.migrations {
        if is_applied(pool, set.namespace, migration.version).await? {
            continue;
        }
        run_migration(pool, set.namespace, migration).await?;
        record_applied(pool, set.namespace, migration).await?;
    }
    Ok(())
}

/// Run a single migration's SQL inside a transaction. Multi-statement bodies are
/// executed via the connection's batch executor. A failure surfaces the
/// migration id so operators can locate the offending file.
async fn run_migration(
    pool: &SqlitePool,
    namespace: &str,
    migration: &SqlMigration,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    tx.execute(migration.sql).await.map_err(|e| {
        sqlx::Error::Configuration(
            format!(
                "migration {namespace}/{name} (v{version}) failed: {e}",
                name = migration.name,
                version = migration.version,
            )
            .into(),
        )
    })?;
    tx.commit().await?;
    Ok(())
}

/// True when `(namespace, version)` is already recorded as applied.
async fn is_applied(pool: &SqlitePool, namespace: &str, version: i64) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace = ? AND version = ?",
    )
    .bind(namespace)
    .bind(version)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Record a migration as applied. `INSERT OR IGNORE` keeps concurrent/repeat
/// runs a no-op.
async fn record_applied(
    pool: &SqlitePool,
    namespace: &str,
    migration: &SqlMigration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO axon_applied_migrations (namespace, version, name) \
         VALUES (?, ?, ?)",
    )
    .bind(namespace)
    .bind(migration.version)
    .bind(migration.name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Assert the set's versions are dense and strictly increasing starting at `1`,
/// so a missing/duplicated migration is caught before any SQL runs.
fn validate_versions(set: &MigrationSet) -> Result<(), sqlx::Error> {
    for (index, migration) in set.migrations.iter().enumerate() {
        let expected = index as i64 + 1;
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

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;

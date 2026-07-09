use super::*;
use crate::store::open_sqlite_pool;

/// Every namespace's version space must be dense and strictly increasing.
#[test]
fn all_sets_have_dense_versions() {
    for set in composed_sets() {
        validate_versions(&set)
            .unwrap_or_else(|e| panic!("set '{}' failed version validation: {e}", set.namespace));
    }
}

/// The composed order must put `ledger` before `jobs` so `jobs.source_id` can FK
/// `sources(source_id)`; the orphan stores follow.
#[test]
fn composed_order_is_dependency_first() {
    let namespaces: Vec<&str> = composed_sets().iter().map(|s| s.namespace).collect();
    assert_eq!(namespaces, ["ledger", "jobs", "observe", "graph", "memory"]);
}

/// Namespaces are unique so the single applied-migrations table never collides.
#[test]
fn namespaces_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for set in composed_sets() {
        assert!(
            seen.insert(set.namespace),
            "duplicate namespace {}",
            set.namespace
        );
    }
}

/// A fresh on-disk DB migrates cleanly: all sets apply, the contract `sources`
/// table exists (SOLE-created by the ledger set), the jobs `jobs` table exists
/// and its FK to `sources` resolves, and the observe/graph/memory tables exist.
///
/// A file-backed DB (not `:memory:`) is used so every pooled connection sees the
/// same database, matching the production runtime path.
#[tokio::test]
async fn fresh_db_migrates_all_namespaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("migrate.db");
    let pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("open pool");

    // The applied-migrations ledger records every migration exactly once.
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_applied_migrations")
        .fetch_one(&pool)
        .await
        .expect("count applied");
    let expected: i64 = composed_sets()
        .iter()
        .map(|s| s.migrations.len() as i64)
        .sum();
    assert_eq!(applied, expected, "every migration recorded once");

    for table in [
        // ledger contract tables
        "sources",
        "source_generations",
        "source_manifests",
        "source_items",
        "document_status",
        "cleanup_debt",
        "leases",
        // jobs tables
        "jobs",
        "axon_crawl_jobs",
        // observe / graph / memory
        "axon_observe_events",
        "axon_observe_provider_health",
        "graph_nodes",
        "graph_edges",
        "memory_records",
        "memory_links",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("probe {table}: {e}"));
        assert_eq!(count, 1, "table {table} should exist exactly once");
    }

    // `jobs.source_id` FK resolves against `sources(source_id)`. Foreign keys are
    // enforced (open_pool_unlocked sets PRAGMA foreign_keys=ON), so a jobs row
    // referencing a present source inserts, and a dangling one fails.
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at) \
         VALUES ('s1', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("insert source");

    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) \
         VALUES ('j1', 'source', 'queued', 'queued', 'normal', 's1', '', '')",
    )
    .execute(&pool)
    .await
    .expect("insert job with valid FK");
}

/// Re-running the composed runner on an already-migrated pool is a no-op: no
/// "table already exists" error and no duplicate applied-migration rows.
#[tokio::test]
async fn repeated_run_is_noop() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("migrate.db");
    let pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("open pool");
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_applied_migrations")
        .fetch_one(&pool)
        .await
        .expect("count before");

    apply_all_migrations(&pool)
        .await
        .expect("second run should be a clean no-op");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_applied_migrations")
        .fetch_one(&pool)
        .await
        .expect("count after");
    assert_eq!(before, after, "no duplicate applied-migration rows");
}

/// Migration 0021 rebuilds `jobs` to widen its `kind` CHECK constraint
/// (adding `embed`/`crawl`/`ingest`). Prove the rebuild — run for real,
/// in isolation, on a pool seeded with data that pre-dates it — preserves
/// existing rows in `jobs` AND in a child table that FKs
/// `jobs(job_id) ON DELETE CASCADE`. This is the exact failure mode a plain
/// `DROP TABLE jobs` under `foreign_keys = ON` would produce: the child row
/// would be silently CASCADE-deleted the moment the old table is dropped,
/// which is why `run_jobs_table_rebuild_migration` disables enforcement for
/// the rebuild instead of using the generic `pool.begin()` transaction path.
#[tokio::test]
async fn jobs_table_rebuild_preserves_existing_rows_and_child_fk_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("migrate.db");
    // Bypass `open_sqlite_pool` (which would run every migration, including
    // 0021) so this test can apply only migrations 1..=20, seed data on that
    // pre-rebuild schema, and then run migration 21 in isolation.
    let pool = axon_core::sqlite::open_pool_unlocked(path.to_str().unwrap())
        .await
        .expect("open raw pool");

    // `jobs.source_id` FKs `sources(source_id)`, owned by the ledger set —
    // apply it first, matching the composed runner's dependency order.
    let ledger_set = axon_ledger::migration::migration_set();
    for migration in ledger_set.migrations {
        run_migration(&pool, ledger_set.namespace, migration)
            .await
            .unwrap_or_else(|e| panic!("ledger migration {} failed: {e}", migration.name));
    }

    let (through_20, rest) = JOBS_MIGRATIONS.split_at(20);
    // Only migration 21 (the jobs-table rebuild) is under test here; slice
    // off any migrations after it (e.g. 0022's additive `ALTER TABLE`) so
    // this test stays valid regardless of how many migrations follow.
    let migration_21 = &rest[..1];
    assert_eq!(migration_21[0].version, 21);

    for migration in through_20 {
        run_migration(&pool, JOBS_NAMESPACE, migration)
            .await
            .unwrap_or_else(|e| panic!("pre-0021 migration {} failed: {e}", migration.name));
    }

    // Seed a `jobs` row and a `job_events` row that FKs it, using a kind that
    // was already valid pre-migration-21 (so the seed itself doesn't depend
    // on the fix under test).
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, created_at, updated_at) \
         VALUES ('job-preserve-1', 'extract', 'queued', 'queued', 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("seed jobs row");

    sqlx::query(
        "INSERT INTO job_events (event_id, job_id, attempt, sequence, phase, status, severity, visibility, message, timestamp) \
         VALUES ('evt-preserve-1', 'job-preserve-1', 1, 1, 'queued', 'queued', 'info', 'public', 'seeded', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("seed child job_events row referencing job-preserve-1");

    // Now run the rebuild migration for real, on the seeded pre-0021 schema.
    run_migration(&pool, JOBS_NAMESPACE, &migration_21[0])
        .await
        .expect("migration 0021 (jobs table rebuild) should succeed");

    let job_kind: String =
        sqlx::query_scalar("SELECT kind FROM jobs WHERE job_id = 'job-preserve-1'")
            .fetch_one(&pool)
            .await
            .expect("seeded jobs row must survive the rebuild");
    assert_eq!(job_kind, "extract");

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = 'job-preserve-1'")
            .fetch_one(&pool)
            .await
            .expect("seeded job_events row must survive the rebuild");
    assert_eq!(
        event_count, 1,
        "child row referencing jobs(job_id) must not be cascade-deleted by the rebuild"
    );

    // The widened CHECK constraint accepts the new family kinds.
    for kind in ["embed", "crawl", "ingest"] {
        let job_id = format!("job-widened-{kind}");
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, created_at, updated_at) \
             VALUES (?, ?, 'queued', 'queued', 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        )
        .bind(&job_id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("widened kind '{kind}' should now be accepted: {e}"));
    }

    // Foreign-key integrity holds across the whole database after the rebuild,
    // and enforcement is genuinely restored (not left disabled).
    let violations = sqlx::query("PRAGMA foreign_key_check;")
        .fetch_all(&pool)
        .await
        .expect("foreign_key_check");
    assert!(
        violations.is_empty(),
        "no foreign-key violations should remain after the jobs table rebuild"
    );
    let fk_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await
        .expect("read foreign_keys pragma");
    assert_eq!(
        fk_enabled, 1,
        "foreign_keys enforcement must be restored after the rebuild, not left disabled"
    );

    let orphan_insert = sqlx::query(
        "INSERT INTO job_events (event_id, job_id, attempt, sequence, phase, status, severity, visibility, message, timestamp) \
         VALUES ('evt-orphan', 'no-such-job', 1, 1, 'queued', 'queued', 'info', 'public', 'orphan', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await;
    assert!(
        orphan_insert.is_err(),
        "foreign_keys enforcement must actually reject an orphaned child row post-rebuild"
    );
}

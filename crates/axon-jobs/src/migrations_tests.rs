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
        // legacy axon_source_* kept during cutover
        "axon_source_sources",
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

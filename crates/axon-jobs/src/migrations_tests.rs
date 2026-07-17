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

#[tokio::test]
async fn pre_cutover_version_one_store_is_rejected_without_mutation() {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("open fixture pool");
    sqlx::raw_sql(include_str!("migrations/fixtures/legacy_jobs_v1.sql"))
        .execute(&pool)
        .await
        .expect("create legacy fixture");

    let error = apply_all_migrations(&pool)
        .await
        .expect_err("legacy version-one store must fail closed");
    let message = error.to_string();
    assert!(message.contains("startup.incompatible_store"), "{message}");
    assert!(message.contains("axon reset"), "{message}");

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("read unchanged table inventory");
    assert_eq!(tables, ["axon_applied_migrations", "jobs"]);
    let receipt_columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('axon_applied_migrations') ORDER BY cid",
    )
    .fetch_all(&pool)
    .await
    .expect("read unchanged receipt columns");
    assert_eq!(
        receipt_columns,
        ["namespace", "version", "name", "applied_at"]
    );
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

    for table in [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
        "axon_ingest_payloads",
        "axon_job_cutover_receipts",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("probe {table}: {e}"));
        assert_eq!(count, 0, "table {table} must not exist in final schema");
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

#[tokio::test]
async fn canonical_store_with_tampered_checksum_is_rejected() {
    let pool = open_sqlite_pool(":memory:")
        .await
        .expect("create canonical store");
    sqlx::query(
        "UPDATE axon_applied_migrations SET checksum = 'tampered' \
         WHERE namespace = 'jobs' AND version = 1",
    )
    .execute(&pool)
    .await
    .expect("tamper receipt");

    let error = apply_all_migrations(&pool)
        .await
        .expect_err("tampered checksum must fail closed");
    assert!(
        error
            .to_string()
            .contains("names, versions, checksums, or epochs"),
        "{error}"
    );
}

#[tokio::test]
async fn canonical_store_with_extra_table_is_rejected() {
    let pool = open_sqlite_pool(":memory:")
        .await
        .expect("create canonical store");
    sqlx::query("CREATE TABLE legacy_extra (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("add legacy table");

    let error = apply_all_migrations(&pool)
        .await
        .expect_err("table drift must fail closed");
    assert!(error.to_string().contains("table inventory"), "{error}");
}

#[tokio::test]
async fn migration_failure_rolls_back_schema_and_receipts_atomically() {
    static BROKEN: &[SqlMigration] = &[SqlMigration {
        version: 1,
        name: "0001_broken",
        sql: "CREATE TABLE partial_write (id TEXT); INVALID SQL",
    }];
    let pool = SqlitePool::connect(":memory:").await.expect("open pool");
    let mut tx = pool.begin().await.expect("begin");
    ensure_applied_table(&mut tx)
        .await
        .expect("create receipt table");
    let error = apply_set(&mut tx, MigrationSet::new("broken", BROKEN))
        .await
        .expect_err("broken migration must fail");
    assert!(error.to_string().contains("migration broken/0001_broken"));
    tx.rollback().await.expect("rollback");

    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("count tables");
    assert_eq!(
        table_count, 0,
        "failed migration must leave no schema writes"
    );
}

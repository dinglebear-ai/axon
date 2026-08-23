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
        "embedding_vector_cache",
        "provider_identity_cache",
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
async fn canonical_epoch_one_store_applies_missing_tail_migrations() {
    static OLD_JOBS: &[SqlMigration] = &[JOBS_MIGRATIONS[0]];
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("epoch-one.db");
    let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("open fixture pool");
    let old_sets = [
        axon_ledger::migration::migration_set(),
        MigrationSet::new(JOBS_NAMESPACE, OLD_JOBS),
        axon_observe::migration::migration_set(),
        axon_graph::migration::migration_set(),
        axon_memory::migration::migration_set(),
    ];
    let mut tx = pool.begin().await.expect("begin epoch-one fixture");
    ensure_applied_table(&mut tx)
        .await
        .expect("create receipt table");
    for set in old_sets {
        apply_set(&mut tx, set).await.expect("apply epoch-one set");
    }
    identity::stamp_schema_epoch(&mut tx)
        .await
        .expect("stamp epoch");
    tx.commit().await.expect("commit epoch-one fixture");

    apply_all_migrations(&pool)
        .await
        .expect("canonical epoch-one store should upgrade in place");

    let scheduler_receipt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations          WHERE namespace = 'jobs' AND version = 2 AND name = '0002_provider_scheduler'",
    )
    .fetch_one(&pool)
    .await
    .expect("read scheduler receipt");
    assert_eq!(scheduler_receipt, 1);
    let performance_receipt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace = 'jobs' AND version = 3 AND name = '0003_provider_scheduler_performance'",
    )
    .fetch_one(&pool)
    .await
    .expect("read scheduler performance receipt");
    assert_eq!(performance_receipt, 1);
    let parser_kind_receipt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations WHERE namespace = 'jobs' AND version = 4 AND name = '0004_provider_scheduler_parser_kind'",
    )
    .fetch_one(&pool)
    .await
    .expect("read scheduler parser-kind receipt");
    assert_eq!(parser_kind_receipt, 1);
    let hot_index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
            'provider_reservations_scheduler_instance_state_idx',
            'provider_reservations_scheduler_instance_sequence_idx',
            'provider_reservations_scheduler_job_state_idx'
        )",
    )
    .fetch_one(&pool)
    .await
    .expect("read scheduler performance indexes");
    assert_eq!(hot_index_count, 3);
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('provider_reservations') ORDER BY cid",
    )
    .fetch_all(&pool)
    .await
    .expect("read reservation columns");
    for required in [
        "capacity_domain",
        "authority_id",
        "enqueue_sequence",
        "effective_priority",
        "attempt",
        "fence",
        "quarantined",
    ] {
        assert!(
            columns.iter().any(|column| column == required),
            "missing upgraded scheduler column {required}"
        );
    }
}

#[tokio::test]
async fn version_five_watch_store_upgrades_with_replay_defaults_and_reopens_idempotently() {
    static JOBS_THROUGH_FIVE: &[SqlMigration] = &[
        SqlMigration {
            version: 1,
            name: "0001_canonical_jobs",
            sql: include_str!("migrations/0001_canonical_jobs.sql"),
        },
        SqlMigration {
            version: 2,
            name: "0002_provider_scheduler",
            sql: include_str!("migrations/0002_provider_scheduler.sql"),
        },
        SqlMigration {
            version: 3,
            name: "0003_provider_scheduler_performance",
            sql: include_str!("migrations/0003_provider_scheduler_performance.sql"),
        },
        SqlMigration {
            version: 4,
            name: "0004_provider_scheduler_parser_kind",
            sql: include_str!("migrations/0004_provider_scheduler_parser_kind.sql"),
        },
        SqlMigration {
            version: 5,
            name: "0005_provider_identity_cache",
            sql: include_str!("migrations/0005_provider_identity_cache.sql"),
        },
    ];
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("version-five-watch.db");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.expect("open fixture pool");
    let old_sets = [
        axon_ledger::migration::migration_set(),
        MigrationSet::new(JOBS_NAMESPACE, JOBS_THROUGH_FIVE),
        axon_observe::migration::migration_set(),
        axon_graph::migration::migration_set(),
        axon_memory::migration::migration_set(),
    ];
    let mut tx = pool.begin().await.expect("begin version-five fixture");
    ensure_applied_table(&mut tx)
        .await
        .expect("create receipt table");
    for set in old_sets {
        apply_set(&mut tx, set)
            .await
            .expect("apply version-five fixture set");
    }
    identity::stamp_schema_epoch(&mut tx)
        .await
        .expect("stamp epoch");
    sqlx::query(
        "INSERT INTO axon_source_watches (
            watch_id, source, source_id, canonical_uri, adapter_name, adapter_version,
            scope, embed, options_json, collection, enabled, every_seconds, next_run_at,
            created_at, updated_at, auth_snapshot_json
         ) VALUES (
            'watch-v5', 'file:///repo', 'source-v5', 'local://repo', 'local', '1',
            'directory', 1, '{\"render\":true}', 'axon', 1, 60, 1700000000000,
            1699999999000, 1699999999000, '{\"transport\":\"panel\",\"principal\":\"operator\"}'
         )",
    )
    .execute(&mut *tx)
    .await
    .expect("populate version-five watch");
    tx.commit().await.expect("commit version-five fixture");
    pool.close().await;

    let pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("upgrade version-five database");
    let row: (String, String, String, String) = sqlx::query_as(
        "SELECT source, options_json, limits_json, metadata_json
         FROM axon_source_watches WHERE watch_id = 'watch-v5'",
    )
    .fetch_one(&pool)
    .await
    .expect("read upgraded watch");
    assert_eq!(row.0, "file:///repo");
    assert_eq!(row.1, r#"{"render":true}"#);
    assert_eq!(row.2, "{}");
    assert_eq!(row.3, "{}");
    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations
         WHERE namespace = 'jobs' AND version = 6 AND name = '0006_watch_request_replay'",
    )
    .fetch_one(&pool)
    .await
    .expect("read version-six receipt");
    assert_eq!(receipt_count, 1);
    let cache_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema \
         WHERE type = 'table' AND name = 'embedding_vector_cache'",
    )
    .fetch_one(&pool)
    .await
    .expect("read upgraded embedding cache table");
    assert_eq!(cache_table_count, 1);
    let cache_receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations \
         WHERE namespace = 'jobs' AND version = 7 AND name = '0007_embedding_vector_cache'",
    )
    .fetch_one(&pool)
    .await
    .expect("read version-seven receipt");
    assert_eq!(cache_receipt_count, 1);
    let expiry_receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations \
         WHERE namespace = 'jobs' AND version = 8 \
         AND name = '0008_embedding_vector_cache_expiry'",
    )
    .fetch_one(&pool)
    .await
    .expect("read version-eight receipt");
    assert_eq!(expiry_receipt_count, 1);
    assert!(
        sqlx::query(
            "UPDATE axon_source_watches SET limits_json = 'not-json' WHERE watch_id = 'watch-v5'"
        )
        .execute(&pool)
        .await
        .is_err(),
        "upgraded replay JSON columns must reject malformed values"
    );

    apply_all_migrations(&pool)
        .await
        .expect("repeat migration is a no-op");
    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations
         WHERE namespace = 'jobs' AND version = 6",
    )
    .fetch_one(&pool)
    .await
    .expect("count idempotent receipt");
    assert_eq!(receipt_count, 1);
    let cache_receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM axon_applied_migrations \
         WHERE namespace = 'jobs' AND version = 7",
    )
    .fetch_one(&pool)
    .await
    .expect("count idempotent cache receipt");
    assert_eq!(cache_receipt_count, 1);
    pool.close().await;

    let reopened = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("reopen upgraded database");
    let defaults: (String, String) = sqlx::query_as(
        "SELECT limits_json, metadata_json FROM axon_source_watches WHERE watch_id = 'watch-v5'",
    )
    .fetch_one(&reopened)
    .await
    .expect("read replay defaults after reopen");
    assert_eq!(defaults, ("{}".to_string(), "{}".to_string()));
}

#[tokio::test]
async fn parser_capacity_domain_is_accepted_after_migration() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('parser-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-0000000000aa', 'source', 'queued', 'queued', 'normal', 'parser-source', '', '')",
    )
    .execute(&pool)
    .await
    .expect("job");
    sqlx::query(
        "INSERT INTO provider_reservations (reservation_id, job_id, provider_kind, priority, requested_units, granted_units, status, updated_at, capacity_domain, instance_id, authority_id, requested_priority, effective_priority, attempt, fence) \
         VALUES ('parser-reservation', '00000000-0000-0000-0000-0000000000aa', 'parser', 'normal', 1, 0, 'queued', datetime('now'), 'parser', 'source-parser', 'test-authority', 'normal', 'normal', 0, 'parser-fence')",
    )
    .execute(&pool)
    .await
    .expect("parser reservation should satisfy provider_kind check");

    let stored: String = sqlx::query_scalar(
        "SELECT provider_kind FROM provider_reservations WHERE reservation_id = 'parser-reservation'",
    )
    .fetch_one(&pool)
    .await
    .expect("parser reservation row");
    assert_eq!(stored, "parser");
}

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
            .contains("migration receipt jobs/1 does not match canonical identity"),
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

#[tokio::test]
async fn upgrade_preflight_allows_missing_tail_schema_but_rejects_unknown_schema() {
    let pool = SqlitePool::connect(":memory:").await.expect("open pool");
    sqlx::query(
        "CREATE TABLE axon_applied_migrations (
            namespace TEXT NOT NULL, version INTEGER NOT NULL, name TEXT NOT NULL,
            checksum TEXT NOT NULL, schema_epoch INTEGER NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (namespace, version)
        )",
    )
    .execute(&pool)
    .await
    .expect("canonical receipt table");
    let mut connection = pool.acquire().await.expect("connection");
    identity::validate_table_subset(&mut connection)
        .await
        .expect("missing canonical tail tables are allowed before mutation");
    identity::validate_foreign_key_subset(&mut connection)
        .await
        .expect("missing canonical tail foreign keys are allowed before mutation");
    drop(connection);

    sqlx::query("CREATE TABLE legacy_extra (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("unknown table");
    let mut connection = pool.acquire().await.expect("connection");
    let error = identity::validate_table_subset(&mut connection)
        .await
        .expect_err("unknown schema must still fail closed");
    assert!(error.to_string().contains("legacy_extra"), "{error}");
}

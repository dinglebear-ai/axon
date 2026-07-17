use axon_api::migration::MigrationSet;
use sha2::{Digest, Sha256};
use sqlx::{Executor, Row, SqliteConnection};
use std::collections::BTreeSet;

pub(super) const SCHEMA_EPOCH: i64 = 1;

const CANONICAL_TABLES: &[&str] = &[
    "axon_applied_migrations",
    "axon_observe_events",
    "axon_observe_heartbeats",
    "axon_observe_provider_health",
    "axon_source_watch_runs",
    "axon_source_watches",
    "cleanup_debt",
    "config_snapshots",
    "document_status",
    "graph_aliases",
    "graph_conflicts",
    "graph_edges",
    "graph_evidence",
    "graph_nodes",
    "job_artifacts",
    "job_attempts",
    "job_events",
    "job_heartbeats",
    "job_stages",
    "jobs",
    "leases",
    "memory_links",
    "memory_records",
    "memory_reinforcement",
    "memory_reviews",
    "provider_reservations",
    "source_generations",
    "source_items",
    "source_manifests",
    "sources",
];

const CANONICAL_FOREIGN_KEYS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "axon_source_watch_runs",
        "watch_id",
        "axon_source_watches",
        "watch_id",
        "CASCADE",
    ),
    (
        "cleanup_debt",
        "source_id",
        "sources",
        "source_id",
        "CASCADE",
    ),
    (
        "document_status",
        "source_id",
        "sources",
        "source_id",
        "CASCADE",
    ),
    (
        "graph_aliases",
        "node_id",
        "graph_nodes",
        "node_id",
        "CASCADE",
    ),
    (
        "graph_edges",
        "from_node_id",
        "graph_nodes",
        "node_id",
        "CASCADE",
    ),
    (
        "graph_edges",
        "to_node_id",
        "graph_nodes",
        "node_id",
        "CASCADE",
    ),
    (
        "graph_evidence",
        "edge_id",
        "graph_edges",
        "edge_id",
        "CASCADE",
    ),
    ("job_artifacts", "job_id", "jobs", "job_id", "CASCADE"),
    ("job_attempts", "job_id", "jobs", "job_id", "CASCADE"),
    ("job_events", "job_id", "jobs", "job_id", "CASCADE"),
    (
        "job_events",
        "stage_id",
        "job_stages",
        "stage_id",
        "SET NULL",
    ),
    ("job_heartbeats", "job_id", "jobs", "job_id", "CASCADE"),
    ("job_stages", "job_id", "jobs", "job_id", "CASCADE"),
    ("jobs", "parent_job_id", "jobs", "job_id", "SET NULL"),
    ("jobs", "root_job_id", "jobs", "job_id", "SET NULL"),
    ("jobs", "source_id", "sources", "source_id", "SET NULL"),
    (
        "jobs",
        "watch_id",
        "axon_source_watches",
        "watch_id",
        "SET NULL",
    ),
    (
        "memory_links",
        "memory_id",
        "memory_records",
        "memory_id",
        "CASCADE",
    ),
    (
        "memory_reinforcement",
        "memory_id",
        "memory_records",
        "memory_id",
        "CASCADE",
    ),
    (
        "memory_reviews",
        "memory_id",
        "memory_records",
        "memory_id",
        "CASCADE",
    ),
    (
        "provider_reservations",
        "job_id",
        "jobs",
        "job_id",
        "CASCADE",
    ),
    (
        "provider_reservations",
        "stage_id",
        "job_stages",
        "stage_id",
        "SET NULL",
    ),
    (
        "source_generations",
        "source_id",
        "sources",
        "source_id",
        "CASCADE",
    ),
    (
        "source_items",
        "generation",
        "source_manifests",
        "generation",
        "CASCADE",
    ),
    (
        "source_items",
        "source_id",
        "source_manifests",
        "source_id",
        "CASCADE",
    ),
    (
        "source_manifests",
        "generation",
        "source_generations",
        "generation",
        "CASCADE",
    ),
    (
        "source_manifests",
        "source_id",
        "source_generations",
        "source_id",
        "CASCADE",
    ),
];

pub(super) fn migration_checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.as_bytes()))
}

pub(super) async fn validate_before_mutation(
    connection: &mut SqliteConnection,
    sets: &[MigrationSet],
) -> Result<bool, sqlx::Error> {
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&mut *connection)
    .await?;
    if table_count == 0 {
        return Ok(true);
    }
    validate_canonical(connection, sets).await?;
    Ok(false)
}

pub(super) async fn stamp_schema_epoch(
    connection: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    connection.execute("PRAGMA user_version = 1").await?;
    Ok(())
}

pub(super) async fn validate_canonical(
    connection: &mut SqliteConnection,
    sets: &[MigrationSet],
) -> Result<(), sqlx::Error> {
    validate_epoch(connection).await?;
    validate_receipts(connection, sets).await?;
    validate_tables(connection).await?;
    validate_foreign_keys(connection).await?;
    Ok(())
}

async fn validate_epoch(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let epoch: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *connection)
        .await?;
    require(
        epoch == SCHEMA_EPOCH,
        format!("schema epoch {epoch}, expected {SCHEMA_EPOCH}"),
    )
}

async fn validate_receipts(
    connection: &mut SqliteConnection,
    sets: &[MigrationSet],
) -> Result<(), sqlx::Error> {
    let columns = pragma_names(
        connection,
        "PRAGMA table_info('axon_applied_migrations')",
        "name",
    )
    .await?;
    let expected_columns = BTreeSet::from([
        "namespace".to_string(),
        "version".to_string(),
        "name".to_string(),
        "checksum".to_string(),
        "schema_epoch".to_string(),
        "applied_at".to_string(),
    ]);
    require(
        columns == expected_columns,
        "migration receipt table shape does not match canonical schema",
    )?;

    let rows = sqlx::query(
        "SELECT namespace, version, name, checksum, schema_epoch FROM axon_applied_migrations",
    )
    .fetch_all(&mut *connection)
    .await?;
    let actual: BTreeSet<_> = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("namespace"),
                row.get::<i64, _>("version"),
                row.get::<String, _>("name"),
                row.get::<String, _>("checksum"),
                row.get::<i64, _>("schema_epoch"),
            )
        })
        .collect();
    let expected: BTreeSet<_> = sets
        .iter()
        .flat_map(|set| {
            set.migrations.iter().map(move |migration| {
                (
                    set.namespace.to_string(),
                    migration.version,
                    migration.name.to_string(),
                    migration_checksum(migration.sql),
                    SCHEMA_EPOCH,
                )
            })
        })
        .collect();
    require(
        actual == expected,
        "migration names, versions, checksums, or epochs do not match canonical schema",
    )
}

async fn validate_tables(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let actual = pragma_names(
        connection,
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        "name",
    )
    .await?;
    let expected = CANONICAL_TABLES
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    require(
        actual == expected,
        "table inventory does not match canonical schema",
    )
}

async fn validate_foreign_keys(connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let mut actual = BTreeSet::new();
    for table in CANONICAL_TABLES {
        let sql = format!("PRAGMA foreign_key_list('{table}')");
        for row in sqlx::query(&sql).fetch_all(&mut *connection).await? {
            actual.insert((
                (*table).to_string(),
                row.get::<String, _>("from"),
                row.get::<String, _>("table").trim_matches('"').to_string(),
                row.get::<String, _>("to"),
                row.get::<String, _>("on_delete").to_uppercase(),
            ));
        }
    }
    let expected = CANONICAL_FOREIGN_KEYS
        .iter()
        .map(|(table, from, target, to, delete)| {
            (
                (*table).to_string(),
                (*from).to_string(),
                (*target).to_string(),
                (*to).to_string(),
                (*delete).to_string(),
            )
        })
        .collect();
    require(
        actual == expected,
        "foreign-key inventory does not match canonical schema",
    )
}

async fn pragma_names(
    connection: &mut SqliteConnection,
    sql: &str,
    column: &str,
) -> Result<BTreeSet<String>, sqlx::Error> {
    Ok(sqlx::query(sql)
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>(column))
        .collect())
}

fn require(condition: bool, detail: impl std::fmt::Display) -> Result<(), sqlx::Error> {
    if condition {
        return Ok(());
    }
    Err(sqlx::Error::Configuration(
        format!(
            "startup.incompatible_store: {detail}; this is not a supported migration source. Run `axon reset --dry-run`, review the plan, then `axon reset --yes`"
        )
        .into(),
    ))
}

//! SQLite side of `axon reset`: inventory (table/row counts) and destructive
//! wipe + fresh re-migration of the single unified jobs DB.

use axon_jobs::migrations::apply_all_migrations;
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, Row, SqlitePool};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqliteSchemaIdentity {
    /// Number of exact `(namespace, version, name)` migration identities.
    pub version: i64,
    /// SHA-256 over migration identities and every composed user-table DDL.
    pub checksum: String,
}

#[derive(Debug, Clone, Default)]
pub struct SqliteTableInventory {
    pub rows: u64,
    pub store: Option<String>,
}

/// Inventory of the unified SQLite database used by the reset planner and
/// doctor store-inventory check.
#[derive(Debug, Clone, Default)]
pub struct SqliteInventory {
    /// The DB file exists on disk.
    pub exists: bool,
    /// Count of user tables (excludes `sqlite_*` internal tables).
    pub table_count: usize,
    /// Total rows across primary content-bearing cutover tables. Best-effort; a
    /// missing table contributes zero.
    pub content_rows: u64,
    /// Content rows grouped by reset's logical SQLite store names.
    pub rows_by_store: BTreeMap<String, u64>,
    /// Every user table in the composed DB, including schema bookkeeping.
    pub tables: BTreeMap<String, SqliteTableInventory>,
    /// Exact identity of the composed cross-crate schema.
    pub schema_identity: SqliteSchemaIdentity,
}

impl SqliteInventory {
    /// True when the DB holds data a reset would destroy. A migrated-but-empty
    /// DB (schema tables present, zero content rows) is the expected fresh
    /// post-cutover state — not "non-empty" — so this keys off content rows,
    /// not table presence.
    #[must_use]
    pub fn non_empty(&self) -> bool {
        self.exists && self.content_rows > 0
    }
}

/// Read-only inventory of the SQLite DB. Never mutates. Opens the DB in
/// read-only mode when it exists so planning cannot create schema as a side
/// effect. Returns a zeroed inventory (not an error) when the file is absent.
pub async fn inventory(path: &Path) -> Result<SqliteInventory, Box<dyn Error>> {
    if !path.exists() {
        return Ok(SqliteInventory::default());
    }
    let connect = format!("sqlite://{}?mode=ro", path.display());
    let pool = SqlitePool::connect(&connect).await?;
    let inv = read_inventory(&pool).await;
    pool.close().await;
    Ok(inv?)
}

async fn read_inventory(pool: &SqlitePool) -> Result<SqliteInventory, sqlx::Error> {
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(pool)
    .await?;

    let tables = inventory_tables(pool).await?;
    let rows_by_store = rows_by_store(&tables);
    let content_rows = rows_by_store.values().copied().sum();
    let schema_identity = schema_identity(pool).await?;

    Ok(SqliteInventory {
        exists: true,
        table_count: table_count.max(0) as usize,
        content_rows,
        rows_by_store,
        tables,
        schema_identity,
    })
}

fn store_for_table(table: &str) -> Option<&'static str> {
    match table {
        "jobs"
        | "job_attempts"
        | "job_stages"
        | "job_events"
        | "job_heartbeats"
        | "job_artifacts"
        | "provider_reservations"
        | "config_snapshots"
        | "axon_observe_events"
        | "axon_observe_heartbeats"
        | "axon_observe_provider_health" => Some("jobs"),
        "sources" | "source_generations" | "source_manifests" | "source_items"
        | "document_status" | "cleanup_debt" | "leases" => Some("ledger"),
        "axon_source_watches" | "axon_source_watch_runs" => Some("watch"),
        "graph_nodes" | "graph_aliases" | "graph_edges" | "graph_evidence" | "graph_conflicts" => {
            Some("graph")
        }
        "memory_records" | "memory_links" | "memory_reinforcement" | "memory_reviews" => {
            Some("memory")
        }
        // Schema bookkeeping is inventoried and checksummed but is not user
        // content, so it does not make a fresh DB appear non-empty.
        "axon_applied_migrations" => None,
        _ => None,
    }
}

async fn inventory_tables(
    pool: &SqlitePool,
) -> Result<BTreeMap<String, SqliteTableInventory>, sqlx::Error> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let mut totals = BTreeMap::new();
    for table in names {
        let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
        let count: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await?;
        totals.insert(
            table.clone(),
            SqliteTableInventory {
                rows: count.max(0) as u64,
                store: if table == "axon_applied_migrations" {
                    None
                } else {
                    Some(store_for_table(&table).unwrap_or("jobs").to_string())
                },
            },
        );
    }
    Ok(totals)
}

fn rows_by_store(tables: &BTreeMap<String, SqliteTableInventory>) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for table in tables.values() {
        if let Some(store) = &table.store {
            *totals.entry(store.clone()).or_default() += table.rows;
        }
    }
    totals
}

async fn schema_identity(pool: &SqlitePool) -> Result<SqliteSchemaIdentity, sqlx::Error> {
    let mut identities: Vec<String> = sqlx::query(
        "SELECT name, COALESCE(sql, '') AS sql FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        format!(
            "table:{}:{}",
            row.get::<String, _>(0),
            row.get::<String, _>(1)
        )
    })
    .collect();

    let migration_table_present = identities
        .iter()
        .any(|identity| identity.starts_with("table:axon_applied_migrations:"));
    let migration_columns: Vec<String> = if migration_table_present {
        sqlx::query_scalar("SELECT name FROM pragma_table_info('axon_applied_migrations')")
            .fetch_all(pool)
            .await?
    } else {
        Vec::new()
    };
    let has_exact_identity = migration_columns.iter().any(|name| name == "checksum")
        && migration_columns.iter().any(|name| name == "schema_epoch");
    let migrations: Vec<(String, i64, String, String, i64)> = if has_exact_identity {
        sqlx::query_as(
            "SELECT namespace, version, name, checksum, schema_epoch \
             FROM axon_applied_migrations ORDER BY namespace, version, name",
        )
        .fetch_all(pool)
        .await?
    } else if migration_table_present {
        sqlx::query_as(
            "SELECT namespace, version, name, '' AS checksum, 0 AS schema_epoch \
             FROM axon_applied_migrations ORDER BY namespace, version, name",
        )
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };
    identities.extend(
        migrations
            .iter()
            .map(|(namespace, version, name, checksum, epoch)| {
                format!("migration:{namespace}:{version}:{name}:{checksum}:{epoch}")
            }),
    );
    let digest = Sha256::digest(identities.join("\n").as_bytes());
    Ok(SqliteSchemaIdentity {
        version: migrations.len() as i64,
        checksum: format!("{digest:x}"),
    })
}

/// Destructively wipe the SQLite DB and re-create fresh schema.
///
/// Acquires SQLite's exclusive locking mode, drops every user table in-place,
/// and runs the composed migration set from scratch. Keeping the same single
/// connection for the whole operation prevents another process from retaining
/// an open handle to a deleted inode (and works on Windows, where an open DB
/// cannot be unlinked).
pub async fn wipe_and_remigrate(path: &Path) -> Result<SqliteSchemaIdentity, Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .pragma("busy_timeout", "0")
        .pragma("locking_mode", "EXCLUSIVE");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    acquire_exclusive_and_drop(&pool).await?;
    apply_all_migrations(&pool).await?;
    let identity = schema_identity(&pool).await?;
    pool.close().await;
    Ok(identity)
}

async fn acquire_exclusive_and_drop(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    connection.execute("PRAGMA foreign_keys = OFF").await?;
    connection
        .execute("BEGIN EXCLUSIVE")
        .await
        .map_err(|error| {
            sqlx::Error::Configuration(
                format!("reset.sqlite_not_exclusive: stop Axon and close all DB users: {error}")
                    .into(),
            )
        })?;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&mut *connection)
    .await?;
    for table in tables {
        let quoted = table.replace('"', "\"\"");
        connection
            .execute(format!("DROP TABLE \"{quoted}\"").as_str())
            .await?;
    }
    connection.execute("COMMIT").await?;
    Ok(())
}

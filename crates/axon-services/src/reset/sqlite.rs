//! SQLite side of `axon reset`: inventory (table/row counts) and destructive
//! wipe + fresh re-migration of the single unified jobs DB.

use axon_core::sqlite::open_pool_unlocked;
use axon_jobs::migrations::apply_all_migrations;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

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
    /// Highest applied migration version recorded in `axon_applied_migrations`.
    pub applied_schema_version: i64,
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

    let applied_schema_version = max_applied_version(pool).await;
    let rows_by_store = count_content_rows(pool).await;
    let content_rows = rows_by_store.values().copied().sum();

    Ok(SqliteInventory {
        exists: true,
        table_count: table_count.max(0) as usize,
        content_rows,
        rows_by_store,
        applied_schema_version,
    })
}

/// Highest `(version)` in `axon_applied_migrations`, or 0 when the table is
/// absent (pre-migration DB) or empty.
async fn max_applied_version(pool: &SqlitePool) -> i64 {
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name = 'axon_applied_migrations'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if has_table == 0 {
        return 0;
    }
    sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM axon_applied_migrations")
        .fetch_one(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// Sum rows in the content-bearing tables that exist. A missing table
/// contributes zero, so this is safe on a partially-migrated DB.
async fn count_content_rows(pool: &SqlitePool) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for (store, table) in [
        ("jobs", "jobs"),
        ("jobs", "job_attempts"),
        ("jobs", "job_stages"),
        ("jobs", "job_events"),
        ("jobs", "job_heartbeats"),
        ("jobs", "job_artifacts"),
        ("jobs", "provider_reservations"),
        ("jobs", "config_snapshots"),
        ("ledger", "sources"),
        ("ledger", "source_generations"),
        ("ledger", "source_manifests"),
        ("ledger", "source_items"),
        ("ledger", "document_status"),
        ("ledger", "cleanup_debt"),
        ("ledger", "leases"),
        ("watch", "axon_source_watches"),
        ("watch", "axon_source_watch_runs"),
        ("graph", "graph_nodes"),
        ("graph", "graph_aliases"),
        ("graph", "graph_edges"),
        ("graph", "graph_evidence"),
        ("graph", "graph_conflicts"),
        ("memory", "axon_memory_nodes"),
        ("memory", "axon_memory_edges"),
        ("memory", "memory_records"),
        ("memory", "memory_links"),
        ("memory", "memory_reinforcement"),
        ("memory", "memory_reviews"),
    ] {
        let count = count_table_if_present(pool, table).await;
        *totals.entry(store.to_string()).or_default() += count;
    }
    totals
}

async fn count_table_if_present(pool: &SqlitePool, table: &str) -> u64 {
    let present: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    if present == 0 {
        return 0;
    }
    // Table name comes from a fixed allowlist above — never user input — so the
    // format! interpolation is not an injection vector (identifiers cannot be
    // bound as parameters in SQLite).
    let sql = format!("SELECT COUNT(*) FROM \"{table}\"");
    let count: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await.unwrap_or(0);
    count.max(0) as u64
}

/// Destructively wipe the SQLite DB and re-create fresh schema.
///
/// Deletes the DB file (and its WAL/SHM sidecars), then opens a new pool which
/// runs the composed migration set from scratch, so every store table
/// (ledger/jobs/observe/graph/memory) is recreated at the current schema.
/// Returns the highest applied migration version after re-migration.
pub async fn wipe_and_remigrate(path: &Path) -> Result<i64, Box<dyn Error>> {
    remove_db_files(path)?;
    let pool = open_pool_unlocked(&path.to_string_lossy()).await?;
    apply_all_migrations(&pool).await?;
    let version = max_applied_version(&pool).await;
    pool.close().await;
    Ok(version)
}

/// Remove the SQLite file plus its `-wal` / `-shm` sidecars if present. Missing
/// files are not an error — a reset on a fresh host is valid.
fn remove_db_files(path: &Path) -> std::io::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let target = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            let mut os = path.as_os_str().to_owned();
            os.push(suffix);
            std::path::PathBuf::from(os)
        };
        match std::fs::remove_file(&target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

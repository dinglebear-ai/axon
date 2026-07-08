//! SQLite side of `axon reset`: inventory (table/row counts) and destructive
//! wipe + fresh re-migration of the single unified jobs DB.

use axon_core::sqlite::open_pool_unlocked;
use axon_jobs::migrations::apply_all_migrations;
use axon_jobs::unified::{
    LegacyJobStoreBlocker, RECEIPT_KIND_LEGACY_RESET, detect_incompatible_legacy_jobs,
    record_cutover_receipt,
};
use sqlx::SqlitePool;
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
    let content_rows = count_content_rows(pool).await;

    Ok(SqliteInventory {
        exists: true,
        table_count: table_count.max(0) as usize,
        content_rows,
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
async fn count_content_rows(pool: &SqlitePool) -> u64 {
    let mut total: u64 = 0;
    for table in [
        "jobs",
        "job_events",
        "job_artifacts",
        "sources",
        "source_generations",
        "source_documents",
        "source_cleanup_debt",
        "code_index_generations",
        "code_index_files",
        "watches",
        "watch_runs",
        "memory_records",
        "memory_edges",
        "graph_nodes",
        "graph_edges",
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ] {
        total = total.saturating_add(count_table_if_present(pool, table).await);
    }
    total
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

/// Best-effort pre-wipe legacy job audit, used only to enrich the cutover
/// receipt message. Read-only; never mutates. Returns `None` on any error
/// (missing file, unreadable DB) rather than blocking the reset on it.
pub async fn detect_legacy_jobs(path: &Path) -> Option<LegacyJobStoreBlocker> {
    if !path.exists() {
        return None;
    }
    let connect = format!("sqlite://{}?mode=ro", path.display());
    let pool = SqlitePool::connect(&connect).await.ok()?;
    let result = detect_incompatible_legacy_jobs(&pool).await.ok().flatten();
    pool.close().await;
    result
}

/// Record a `legacy_reset` cutover receipt in the freshly re-migrated DB.
/// Called after [`wipe_and_remigrate`] so future unified-worker starts (and
/// `detect_incompatible_legacy_jobs`) have an auditable record of the reset,
/// even though the wipe itself already dropped any legacy rows.
pub async fn record_legacy_reset_receipt(path: &Path, message: &str) -> Result<(), Box<dyn Error>> {
    let pool = open_pool_unlocked(&path.to_string_lossy()).await?;
    let result = record_cutover_receipt(&pool, RECEIPT_KIND_LEGACY_RESET, message).await;
    pool.close().await;
    result.map_err(|e| e.to_string().into())
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

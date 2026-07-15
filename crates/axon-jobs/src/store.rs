use axon_core::config::Config;
use sqlx::SqlitePool;
use std::path::Path;

pub use axon_core::sqlite::{
    diagnostics as sqlite_diagnostics, readiness as sqlite_readiness,
    record_runtime_error as record_sqlite_runtime_error, recover_corrupted_database,
    reset_runtime_health_for_tests as reset_sqlite_runtime_health_for_tests,
};
pub const RECLAIMED_ERROR_TEXT: &str = "reclaimed stale running job";

/// Open a SQLite pool, enable WAL mode, and run all migrations.
///
/// Pass `":memory:"` for in-memory databases (tests).
pub async fn open_sqlite_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let active_lock = if path == ":memory:" {
        None
    } else {
        axon_core::sqlite::acquire_active_db_lock(Path::new(path))?
    };
    let pool = open_sqlite_pool_unlocked(path).await?;
    axon_core::sqlite::register_active_db_lock(active_lock)?;

    Ok(pool)
}

async fn open_sqlite_pool_unlocked(path: &str) -> Result<SqlitePool, sqlx::Error> {
    // Hardened connect + pragmas + after_release scrub live in axon-core; the
    // composed cross-crate migration runner (see `crate::migrations`) runs every
    // crate's migration set — ledger, jobs, observe, graph, memory — against
    // this ONE unified pool in dependency order. This is the single place the
    // shared runtime DB is migrated; the ledger owns the contract tables so
    // jobs migration 0017 no longer duplicates them.
    let pool = axon_core::sqlite::open_pool_unlocked(path).await?;

    crate::migrations::apply_all_migrations(&pool).await?;

    Ok(pool)
}

/// Reclaim stale watch leases from a previous crashed process.
///
/// Clears `lease_expires_at` for any legacy `axon_watch_defs` row or canonical
/// `axon_source_watches` row whose lease has already expired so the scheduler
/// can re-acquire it immediately.
pub async fn reclaim_stale_watch_leases(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let now = now_ms();
    let legacy = sqlx::query(
        "UPDATE axon_watch_defs SET lease_expires_at = NULL WHERE lease_expires_at < ?",
    )
    .bind(now)
    .execute(pool)
    .await?;
    let source = sqlx::query(
        "UPDATE axon_source_watches SET lease_expires_at = NULL WHERE lease_expires_at < ?",
    )
    .bind(now)
    .execute(pool)
    .await?;
    Ok(legacy.rows_affected() + source.rows_affected())
}

/// Checkpoint all WAL frames into the main database file and close the pool.
///
/// Call this on graceful shutdown before dropping the pool. A TRUNCATE
/// checkpoint moves every WAL frame into the main database and resets the WAL
/// to zero bytes — if the process is then SIGKILL'd there is nothing left to
/// corrupt. Without this, an unkilled checkpoint mid-write is the primary
/// cause of `database disk image is malformed` errors on restart.
///
/// Non-fatal: logs warnings on failure but does not propagate the error, since
/// the pool is being closed regardless.
pub async fn checkpoint_and_close(pool: &SqlitePool) {
    match sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
    {
        Ok(_) => tracing::info!("jobs: WAL checkpoint complete"),
        Err(e) => tracing::warn!(error = %e, "jobs: WAL checkpoint failed on shutdown"),
    }
    pool.close().await;
}

/// Like `open_sqlite_pool`, but recovers automatically if the database is
/// corrupted (`SQLITE_CORRUPT` / code 11).
///
/// On corruption: renames `jobs.db` → `jobs.db.corrupted.<timestamp>`, then
/// opens a fresh database. Job history is lost but Qdrant vector data is
/// unaffected. Logs a `tracing::error!` so the operator sees it.
pub async fn open_sqlite_pool_or_recover(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let active_lock = if path == ":memory:" {
        None
    } else {
        axon_core::sqlite::acquire_active_db_lock(Path::new(path))?
    };
    match open_sqlite_pool_unlocked(path).await {
        Ok(pool) => {
            // Quick integrity probe — catches corruption that slipped past the
            // open (e.g. partial WAL frames from a prior SIGKILL).
            let corrupt = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .map(|s| s != "ok")
                .unwrap_or(false);
            if !corrupt {
                axon_core::sqlite::register_active_db_lock(active_lock)?;
                return Ok(pool);
            }
            pool.close().await;
            drop(active_lock);
            recover_corrupted_database(Path::new(path), "quick_check detected corruption")?;
            open_sqlite_pool(path).await
        }
        Err(e) => {
            drop(active_lock);
            let is_corrupt = e.to_string().contains("malformed")
                || e.to_string().contains("corrupt")
                || e.to_string().contains("code: 11");
            if is_corrupt && path != ":memory:" {
                recover_corrupted_database(Path::new(path), &e.to_string())?;
                open_sqlite_pool(path).await
            } else {
                Err(e)
            }
        }
    }
}

/// Open a SQLite pool from the path stored in `cfg.sqlite_path`.
///
/// Shared by `services/jobs.rs` and `jobs/watch.rs` to avoid duplicating
/// the `open_sqlite_pool(&cfg.sqlite_path.to_string_lossy())` call pattern.
pub async fn open_config_pool(cfg: &Config) -> Result<SqlitePool, sqlx::Error> {
    open_sqlite_pool_or_recover(&cfg.sqlite_path.to_string_lossy()).await
}

/// Current time as Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// The unified job table now owns queue state; this helper remains a harmless
/// advisory for older status callers that only need a "queue busy?" number.
pub async fn count_pending_jobs(sqlite_path: &Path) -> i64 {
    let _ = sqlite_path;
    0
}

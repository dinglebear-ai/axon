use axon_core::config::Config;
use sqlx::SqlitePool;
use std::path::Path;

mod reclaim;
pub use axon_core::sqlite::{
    diagnostics as sqlite_diagnostics, readiness as sqlite_readiness,
    record_runtime_error as record_sqlite_runtime_error, recover_corrupted_database,
    reset_runtime_health_for_tests as reset_sqlite_runtime_health_for_tests,
};
pub use reclaim::RECLAIMED_ERROR_TEXT;
pub use reclaim::{
    ReclaimedJob, ReclaimedJobs, reclaim_stale_running_jobs, reclaim_stale_running_jobs_detailed,
    reclaim_stale_running_jobs_for_table, reclaim_stale_running_jobs_for_table_ids,
    reclaim_stale_running_jobs_for_table_jobs,
};

#[cfg(test)]
pub(crate) use axon_core::sqlite::{
    acquire_recovery_lock, active_db_lock_count_for_tests, active_lock_path, hold_active_db_lock,
    open_lock_file,
};

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
    // Hardened connect + pragmas + after_release scrub live in axon-core; this
    // crate owns the jobs migrations run on top.
    let pool = axon_core::sqlite::open_pool_unlocked(path).await?;

    sqlx::migrate!("src/migrations")
        .run(&pool)
        .await
        .map_err(|e| {
            if matches!(e, sqlx::migrate::MigrateError::VersionMissing(_)) {
                sqlx::Error::Configuration(
                    format!(
                        "{e}\n\nThe database was created by a newer version of axon that \
                         this binary does not know about. Upgrade axon to match the \
                         database, or delete the jobs database to start fresh."
                    )
                    .into(),
                )
            } else {
                sqlx::Error::Configuration(e.into())
            }
        })?;

    Ok(pool)
}

/// Reclaim stale watch leases from a previous crashed process.
///
/// Clears `lease_expires_at` for any `axon_watch_defs` row whose lease has
/// already expired so the scheduler can re-acquire them immediately.
pub async fn reclaim_stale_watch_leases(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let now = now_ms();
    let result = sqlx::query(
        "UPDATE axon_watch_defs SET lease_expires_at = NULL WHERE lease_expires_at < ?",
    )
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
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

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// Count all pending jobs across the four job tables. Best-effort: returns 0
/// if the DB file does not exist yet, cannot be opened, or a table is missing
/// (fresh install before the first schema migration).
///
/// SAFETY: every table name below is a compile-time `&'static str` from a
/// closed set; no caller-controlled value reaches the SQL string.
pub async fn count_pending_jobs(sqlite_path: &Path) -> i64 {
    if !sqlite_path.exists() {
        return 0;
    }
    let path_str = sqlite_path.to_string_lossy();
    let pool = match open_sqlite_pool(&path_str).await {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let tables = [
        "axon_crawl_jobs",
        "axon_embed_jobs",
        "axon_extract_jobs",
        "axon_ingest_jobs",
    ];
    let mut total: i64 = 0;
    for table in &tables {
        let query = format!("SELECT COUNT(*) FROM {table} WHERE status='pending'");
        let count: i64 = sqlx::query_scalar(&query)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
        total += count;
    }
    total
}

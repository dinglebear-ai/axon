use axon_core::config::Config;
use sqlx::SqlitePool;

mod reclaim;
pub use reclaim::RECLAIMED_ERROR_TEXT;
pub use reclaim::{
    ReclaimedJob, ReclaimedJobs, reclaim_stale_running_jobs, reclaim_stale_running_jobs_detailed,
    reclaim_stale_running_jobs_for_table, reclaim_stale_running_jobs_for_table_ids,
    reclaim_stale_running_jobs_for_table_jobs,
};

/// Open a SQLite pool, enable WAL mode, and run all migrations.
///
/// Pass `":memory:"` for in-memory databases (tests).
pub async fn open_sqlite_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    // Hardened connect + pragmas + after_release scrub live in axon-core; this
    // crate owns the jobs migrations run on top.
    let pool = axon_core::sqlite::open_pool(path).await?;

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
    match open_sqlite_pool(path).await {
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
                return Ok(pool);
            }
            pool.close().await;
            tracing::error!(path, "jobs: quick_check detected corruption — recovering");
            rename_corrupted(path);
            open_sqlite_pool(path).await
        }
        Err(e) => {
            let is_corrupt = e.to_string().contains("malformed")
                || e.to_string().contains("corrupt")
                || e.to_string().contains("code: 11");
            if is_corrupt && path != ":memory:" {
                tracing::error!(path, error = %e, "jobs: database corrupt at open — recovering");
                rename_corrupted(path);
                open_sqlite_pool(path).await
            } else {
                Err(e)
            }
        }
    }
}

fn rename_corrupted(path: &str) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dest = format!("{path}.corrupted.{ts}");
    if let Err(e) = std::fs::rename(path, &dest) {
        tracing::warn!(src = path, dst = dest, error = %e, "jobs: could not rename corrupted db");
    } else {
        tracing::info!(src = path, dst = dest, "jobs: renamed corrupted db");
    }
    // Also remove WAL/SHM sidecars so the fresh db starts clean.
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{path}{suffix}");
        let _ = std::fs::remove_file(&sidecar);
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
pub async fn count_pending_jobs(sqlite_path: &std::path::Path) -> i64 {
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

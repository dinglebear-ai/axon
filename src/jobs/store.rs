use crate::core::config::Config;
use sqlx::sqlite::SqliteConnection;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

mod reclaim;
pub(crate) use reclaim::RECLAIMED_ERROR_TEXT;
pub use reclaim::{
    ReclaimedJob, ReclaimedJobs, reclaim_stale_running_jobs, reclaim_stale_running_jobs_detailed,
    reclaim_stale_running_jobs_for_table, reclaim_stale_running_jobs_for_table_ids,
    reclaim_stale_running_jobs_for_table_jobs,
};

/// Scrub any dangling transaction from a connection before it re-enters the
/// pool's idle queue. Wired as the pool's `after_release` hook.
///
/// Every transactional path in axon uses a manual `BEGIN IMMEDIATE` on a raw
/// pooled connection, not sqlx's `Transaction` RAII guard. sqlx does not track
/// manual transactions, so a connection dropped between `BEGIN IMMEDIATE` and
/// its matching `COMMIT`/`ROLLBACK` returns to the pool STILL IN A TRANSACTION,
/// poisoning that slot: the next checkout's `BEGIN IMMEDIATE` fails ("cannot
/// start a transaction within a transaction"), and enough poisoned slots starve
/// `pool.acquire()` until workers silently stop claiming jobs (a confirmed
/// production incident). Rolling back on release scrubs the slot first.
///
/// A `ROLLBACK` with no active transaction errors in SQLite ("cannot rollback -
/// no transaction is active") — that is the expected, harmless case, so the
/// connection is kept (`Ok(true)`). Any *other* rollback failure means the slot
/// may still be poisoned, so it is evicted (`Ok(false)`) instead of returned to
/// the idle queue: per sqlx 0.8 semantics only `Ok(true)` keeps the connection,
/// while `Ok(false)`/`Err` close it and let a waiter open a fresh one.
pub(crate) async fn rollback_on_release(conn: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    match sqlx::query("ROLLBACK").execute(&mut *conn).await {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(db)) if db.message().contains("no transaction is active") => {
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(error = %e, "store: after_release ROLLBACK failed; evicting connection");
            Ok(false)
        }
    }
}

/// Open a SQLite pool, enable WAL mode, and run all migrations.
///
/// Pass `":memory:"` for in-memory databases (tests).
pub async fn open_sqlite_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    if path != ":memory:"
        && let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        // Use ensure_private_dir (mode 0o700) so SQLite WAL/SHM files —
        // which inherit umask defaults and may contain credential
        // snapshots from job payloads — are not group/world-readable
        // on multi-user hosts.
        //
        // Failure policy: if the parent is under ~/.axon/, hard-fail —
        // SQLite would otherwise create the dir at default umask
        // and silently expose secrets. For paths the operator chose
        // explicitly (AXON_SQLITE_PATH=/var/lib/axon/...), warn-and-
        // continue so non-secret operator-managed locations still work.
        if let Err(e) = crate::core::paths::ensure_private_dir_async(parent.to_path_buf()).await {
            let parent_under_axon_home =
                crate::core::paths::axon_home_dir().is_some_and(|home| parent.starts_with(&home));
            if parent_under_axon_home {
                return Err(sqlx::Error::Configuration(
                    format!(
                        "jobs: refusing to open SQLite at {} because parent dir {} could not be created at 0o700: {e}",
                        path,
                        parent.display()
                    )
                    .into(),
                ));
            }
            tracing::warn!(path = %parent.display(), error = %e, "jobs: failed to create SQLite parent dir at 0o700");
        }
    }

    let connect_str = if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{}?mode=rwc", path)
    };

    // Pre-create the file at 0o600 before SQLite connects to eliminate the TOCTOU
    // window where the DB is world-readable (default umask is typically 0644).
    // SQLite opens the existing file rather than creating a new one when the path exists.
    #[cfg(unix)]
    if path != ":memory:" {
        use std::os::unix::fs::OpenOptionsExt;
        if let Err(e) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            tracing::warn!(path = %path, error = %e, "jobs: failed to pre-create SQLite file at 0o600; DB may be world-readable until chmod runs");
        }
    }

    let opts: SqliteConnectOptions = connect_str.parse()?;
    let opts = opts
        .pragma("journal_mode", "WAL")
        // NORMAL is the recommended synchronous setting for WAL — durable per
        // commit without the extra checkpoint fsync that FULL (default) adds.
        .pragma("synchronous", "NORMAL")
        // Raise auto-checkpoint threshold from 1000 pages (~4 MB) to 4000
        // (~16 MB). Fewer checkpoints = fewer windows where SIGKILL can corrupt
        // the main database mid-checkpoint. We flush explicitly on shutdown via
        // `checkpoint_and_close`.
        .pragma("wal_autocheckpoint", "4000")
        // 64 MB page cache. Reduces I/O contention when many workers read/write
        // the same hot pages concurrently.
        .pragma("cache_size", "-65536")
        .pragma("temp_store", "MEMORY")
        .pragma("busy_timeout", "30000")
        .pragma("foreign_keys", "ON");

    // 4 connections: enough for concurrent readers in WAL mode; SQLite only
    // allows one writer at a time so more connections beyond this are just
    // queueing overhead.
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(60))
        // Scrub dangling manual transactions before a connection re-enters the
        // pool so a `BEGIN IMMEDIATE` dropped without COMMIT/ROLLBACK cannot
        // poison the slot. See `rollback_on_release`.
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect_with(opts)
        .await?;

    sqlx::migrate!("src/jobs/migrations")
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
pub(crate) async fn open_config_pool(cfg: &Config) -> Result<SqlitePool, sqlx::Error> {
    open_sqlite_pool_or_recover(&cfg.sqlite_path.to_string_lossy()).await
}

/// Current time as Unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

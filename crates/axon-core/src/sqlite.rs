//! Shared secure SQLite pool opener.
//!
//! Owns the connection hardening (0o700 parent dir, 0o600 pre-create to close
//! the world-readable TOCTOU window) and the WAL/pragma tuning that every Axon
//! SQLite database needs. It deliberately does **not** run any migrations —
//! migration ownership stays with each DB-owning crate (e.g. `axon-jobs` runs
//! the jobs migrations after calling [`open_pool`]). This lets read-only callers
//! (stats) open an existing database without depending on the jobs crate.

use sqlx::sqlite::SqliteConnection;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

/// Scrub any dangling transaction from a connection before it re-enters the
/// pool's idle queue. Wired as the pool's `after_release` hook.
///
/// Every transactional path in axon uses a manual `BEGIN IMMEDIATE` on a raw
/// pooled connection, not sqlx's `Transaction` RAII guard. A connection dropped
/// between `BEGIN IMMEDIATE` and its matching `COMMIT`/`ROLLBACK` returns to the
/// pool STILL IN A TRANSACTION, poisoning that slot. Rolling back on release
/// scrubs the slot first. A `ROLLBACK` with no active transaction is the
/// expected, harmless case (`Ok(true)`); any other failure evicts the
/// connection (`Ok(false)`).
pub async fn rollback_on_release(conn: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
    match sqlx::query("ROLLBACK").execute(&mut *conn).await {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(db)) if db.message().contains("no transaction is active") => {
            Ok(true)
        }
        Err(e) => {
            tracing::warn!(error = %e, "sqlite: after_release ROLLBACK failed; evicting connection");
            Ok(false)
        }
    }
}

/// Open a hardened SQLite pool with WAL mode and Axon's standard pragmas.
///
/// Does not run migrations. Pass `":memory:"` for in-memory databases (tests).
pub async fn open_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
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
        if let Err(e) = crate::paths::ensure_private_dir_async(parent.to_path_buf()).await {
            let parent_under_axon_home =
                crate::paths::axon_home_dir().is_some_and(|home| parent.starts_with(&home));
            if parent_under_axon_home {
                return Err(sqlx::Error::Configuration(
                    format!(
                        "sqlite: refusing to open SQLite at {} because parent dir {} could not be created at 0o700: {e}",
                        path,
                        parent.display()
                    )
                    .into(),
                ));
            }
            tracing::warn!(path = %parent.display(), error = %e, "sqlite: failed to create SQLite parent dir at 0o700");
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
            tracing::warn!(path = %path, error = %e, "sqlite: failed to pre-create SQLite file at 0o600; DB may be world-readable until chmod runs");
        }
    }

    let opts: SqliteConnectOptions = connect_str.parse()?;
    let opts = opts
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .pragma("wal_autocheckpoint", "4000")
        .pragma("cache_size", "-65536")
        .pragma("temp_store", "MEMORY")
        .pragma("busy_timeout", "30000")
        .pragma("foreign_keys", "ON");

    SqlitePoolOptions::new()
        .max_connections(4)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect_with(opts)
        .await
}

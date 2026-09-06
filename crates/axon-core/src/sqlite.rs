//! Shared secure SQLite pool opener.
//!
//! Owns the connection hardening (0o700 parent dir, 0o600 pre-create to close
//! the world-readable TOCTOU window) and the WAL/pragma tuning that every Axon
//! SQLite database needs. It deliberately does **not** run any migrations —
//! migration ownership stays with each DB-owning crate (e.g. `axon-jobs` runs
//! the jobs migrations after calling [`open_pool`]). This lets read-only callers
//! (stats) open an existing database without depending on the jobs crate.

mod busy_retry;
mod immediate_tx;
pub use busy_retry::{is_retryable_busy, message_is_retryable_busy, retry_on, with_busy_retry};
pub use immediate_tx::ImmediateTx;

use sqlx::sqlite::SqliteConnection;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// Keep lock handles alive for the process lifetime so a short-lived Axon
// command cannot rename a SQLite database while another Axon process has it open.
static ACTIVE_DB_LOCKS: OnceLock<Mutex<Vec<(PathBuf, File)>>> = OnceLock::new();

#[derive(Debug, Default, Clone)]
struct SqliteRuntimeHealth {
    ioerr_count: u64,
    last_error: Option<String>,
    last_error_at_ms: Option<i64>,
}

static SQLITE_RUNTIME_HEALTH: OnceLock<Mutex<SqliteRuntimeHealth>> = OnceLock::new();

// Match the default unified worker fan-out. Source data-plane work is further
// bounded by `axon-services` so one slot remains available for scheduler and
// heartbeat/control-plane traffic under load.
const DEFAULT_SQLITE_POOL_CONNECTIONS: u32 = 8;

pub type ActiveDbLock = Option<(PathBuf, File)>;

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

/// Keep WAL and shared-memory sidecars linked while pools come and go.
///
/// Axon intentionally opens the same database from several short-lived CLI
/// processes while a long-lived worker is active. Without
/// `SQLITE_FCNTL_PERSIST_WAL`, a closing connection may unlink `-wal`/`-shm`;
/// an already-open worker then continues against the orphaned inodes while a
/// new CLI connection reads a newly-created WAL generation. That split-brain
/// leaves terminal worker writes invisible to `status`/`jobs get`.
///
/// SQLx exposes the raw handle behind an exclusive async guard specifically
/// for SQLite file controls. Pin `libsqlite3-sys` to SQLx 0.8's exact version
/// in this crate so the handle type and constants cannot drift independently.
#[allow(unsafe_code)]
async fn enable_persistent_wal(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    let mut handle = conn.lock_handle().await?;
    let mut enabled = 1i32;
    // SAFETY: `lock_handle()` exclusively owns SQLite's connection worker for
    // this call; `main` and `enabled` remain valid for its synchronous duration.
    let result = unsafe {
        libsqlite3_sys::sqlite3_file_control(
            handle.as_raw_handle().as_ptr(),
            c"main".as_ptr(),
            libsqlite3_sys::SQLITE_FCNTL_PERSIST_WAL,
            std::ptr::from_mut(&mut enabled).cast(),
        )
    };
    if result == libsqlite3_sys::SQLITE_OK {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(format!(
            "sqlite: enabling persistent WAL sidecars failed with code {result}"
        )))
    }
}

pub fn active_lock_path(path: &Path) -> PathBuf {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".active.lock");
    PathBuf::from(lock_path)
}

fn sqlite_config_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Configuration(message.into().into())
}

pub fn open_lock_file(path: &Path) -> Result<File, sqlx::Error> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(active_lock_path(path))
        .map_err(|e| {
            sqlite_config_error(format!(
                "sqlite: failed to open active-owner lock for {}: {e}",
                path.display()
            ))
        })
}

fn map_lock_error(err: std::io::Error, path: &Path, purpose: &str) -> sqlx::Error {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        if purpose == "recovery" {
            return sqlite_config_error(format!(
                "sqlite: refusing recovery for {} because an active Axon process owns the database; stop the active service before recovering",
                path.display()
            ));
        }
        return sqlite_config_error(format!(
            "sqlite: refusing to open {} because database recovery is already in progress",
            path.display()
        ));
    }
    sqlite_config_error(format!(
        "sqlite: failed to acquire {purpose} lock for {}: {err}",
        path.display()
    ))
}

fn active_db_lock_registered(lock_path: &Path) -> Result<bool, sqlx::Error> {
    let locks = ACTIVE_DB_LOCKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map_err(|_| sqlite_config_error("sqlite: active-owner lock registry poisoned"))?;
    Ok(locks.iter().any(|(existing, _)| existing == lock_path))
}

pub fn acquire_active_db_lock(path: &Path) -> Result<ActiveDbLock, sqlx::Error> {
    let lock_path = active_lock_path(path);
    if active_db_lock_registered(&lock_path)? {
        return Ok(None);
    }

    let file = open_lock_file(path)?;
    file.try_lock_shared()
        .map_err(|err| map_lock_error(err.into(), path, "active-owner"))?;
    Ok(Some((lock_path, file)))
}

pub fn register_active_db_lock(lock: ActiveDbLock) -> Result<(), sqlx::Error> {
    let Some((lock_path, file)) = lock else {
        return Ok(());
    };
    let mut locks = ACTIVE_DB_LOCKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map_err(|_| sqlite_config_error("sqlite: active-owner lock registry poisoned"))?;
    if locks.iter().any(|(existing, _)| existing == &lock_path) {
        return Ok(());
    }

    locks.push((lock_path, file));
    Ok(())
}

pub fn hold_active_db_lock(path: &Path) -> Result<(), sqlx::Error> {
    let lock = acquire_active_db_lock(path)?;
    register_active_db_lock(lock)
}

pub fn acquire_recovery_lock(path: &Path) -> Result<File, sqlx::Error> {
    if active_db_lock_registered(&active_lock_path(path))? {
        return Err(sqlite_config_error(format!(
            "sqlite: refusing recovery for {} because this Axon process owns the database; close the pool before recovering",
            path.display()
        )));
    }
    let file = open_lock_file(path)?;
    file.try_lock()
        .map_err(|err| map_lock_error(err.into(), path, "recovery"))?;
    Ok(file)
}

pub fn recover_corrupted_database(path: &Path, reason: &str) -> Result<(), sqlx::Error> {
    if path.as_os_str() == ":memory:" {
        return Err(sqlx::Error::Configuration(
            format!("sqlite: in-memory database is corrupt: {reason}").into(),
        ));
    }

    let _recovery_lock = acquire_recovery_lock(path)?;
    tracing::error!(path = %path.display(), reason, "sqlite: database corrupt; recovering");
    preserve_corrupted_database(path)
}

pub fn record_runtime_error(error: impl Display) {
    let message = error.to_string();
    if !is_sqlite_ioerr_message(&message) {
        return;
    }
    let mut health = SQLITE_RUNTIME_HEALTH
        .get_or_init(|| Mutex::new(SqliteRuntimeHealth::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    health.ioerr_count = health.ioerr_count.saturating_add(1);
    health.last_error = Some(message);
    health.last_error_at_ms = Some(now_ms());
}

fn is_sqlite_ioerr_message(message: &str) -> bool {
    message.contains("SQLITE_IOERR")
        || message.contains("disk I/O error")
        || message.contains("code: 522")
}

fn sqlite_runtime_health() -> SqliteRuntimeHealth {
    SQLITE_RUNTIME_HEALTH
        .get_or_init(|| Mutex::new(SqliteRuntimeHealth::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn active_owner_observation(path: &Path) -> (bool, Option<String>) {
    let active_lock = active_lock_path(path);
    if active_db_lock_registered(&active_lock).unwrap_or(false) {
        return (true, None);
    }
    if !active_lock.exists() {
        return (false, None);
    }
    let file = match OpenOptions::new().read(true).write(true).open(&active_lock) {
        Ok(file) => file,
        Err(err) => return (false, Some(err.to_string())),
    };
    match file.try_lock() {
        Ok(()) => (false, None),
        Err(err) => {
            let err: std::io::Error = err.into();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                (true, None)
            } else {
                (false, Some(err.to_string()))
            }
        }
    }
}

pub fn readiness(path: &Path) -> serde_json::Value {
    let exists = path.exists();
    let runtime = sqlite_runtime_health();
    let active_lock = active_lock_path(path);
    let active_lock_file_exists = active_lock.exists();
    let (active_owner_observed, active_owner_probe_error) = active_owner_observation(path);
    let ok = runtime.ioerr_count == 0 && (!exists || active_owner_observed);

    serde_json::json!({
        "ok": ok,
        "exists": exists,
        "path": path.display().to_string(),
        "check": "runtime",
        "active_lock_path": active_lock.display().to_string(),
        "active_lock_exists": active_lock_file_exists,
        "active_lock_file_exists": active_lock_file_exists,
        "active_owner_observed": active_owner_observed,
        "active_owner_probe_error": active_owner_probe_error,
        "runtime_ioerr_count": runtime.ioerr_count,
        "runtime_last_error": runtime.last_error,
        "runtime_last_error_at_ms": runtime.last_error_at_ms,
    })
}

pub async fn diagnostics(path: &Path) -> serde_json::Value {
    let exists = path.exists();
    let (quick_check, quick_check_ok, quick_check_error) = if exists {
        sqlite_quick_check_readonly(path).await
    } else {
        ("not_created".to_string(), true, None)
    };
    let (corrupted_count, latest_corrupted_path) = corrupted_sidecars(path);
    let runtime = sqlite_runtime_health();
    let ok = quick_check_ok && runtime.ioerr_count == 0;
    let active_lock = active_lock_path(path);
    let active_lock_file_exists = active_lock.exists();
    let (active_owner_observed, active_owner_probe_error) = active_owner_observation(path);

    serde_json::json!({
        "ok": ok,
        "exists": exists,
        "path": path.display().to_string(),
        "quick_check": quick_check,
        "quick_check_error": quick_check_error,
        "active_lock_path": active_lock.display().to_string(),
        "active_lock_exists": active_lock_file_exists,
        "active_lock_file_exists": active_lock_file_exists,
        "active_owner_observed": active_owner_observed,
        "active_owner_probe_error": active_owner_probe_error,
        "corrupted_count": corrupted_count,
        "latest_corrupted_path": latest_corrupted_path.map(|p| p.display().to_string()),
        "runtime_ioerr_count": runtime.ioerr_count,
        "runtime_last_error": runtime.last_error,
        "runtime_last_error_at_ms": runtime.last_error_at_ms,
    })
}

async fn sqlite_quick_check_readonly(path: &Path) -> (String, bool, Option<String>) {
    let connect_str = format!("sqlite://{}?mode=ro", path.display());
    let opts: SqliteConnectOptions = match connect_str.parse::<SqliteConnectOptions>() {
        Ok(opts) => opts.pragma("busy_timeout", "2000"),
        Err(err) => return ("error".to_string(), false, Some(err.to_string())),
    };
    let pool = match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
    {
        Ok(pool) => pool,
        Err(err) => return ("error".to_string(), false, Some(err.to_string())),
    };
    let result = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_optional(&pool)
        .await;
    pool.close().await;
    match result {
        Ok(Some(value)) => {
            let ok = value == "ok";
            (value, ok, None)
        }
        Ok(None) => ("missing".to_string(), false, None),
        Err(err) => ("error".to_string(), false, Some(err.to_string())),
    }
}

fn corrupted_sidecars(path: &Path) -> (usize, Option<PathBuf>) {
    let Some(parent) = path.parent() else {
        return (0, None);
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return (0, None);
    };
    let prefix = format!("{file_name}.corrupted.");
    let mut matches: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                matches.push((modified, entry.path()));
            }
        }
    }
    matches.sort_by(|(left_time, left_path), (right_time, right_path)| {
        left_time
            .cmp(right_time)
            .then_with(|| left_path.cmp(right_path))
    });
    let count = matches.len();
    let latest = matches.pop().map(|(_, path)| path);
    (count, latest)
}

/// Open a hardened SQLite pool with WAL mode and Axon's standard pragmas.
///
/// Does not run migrations. Pass `":memory:"` for in-memory databases (tests).
pub async fn open_pool(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let active_lock = if path == ":memory:" {
        None
    } else {
        acquire_active_db_lock(Path::new(path))?
    };
    let pool = open_pool_unlocked(path).await?;
    register_active_db_lock(active_lock)?;
    Ok(pool)
}

pub async fn open_pool_unlocked(path: &str) -> Result<SqlitePool, sqlx::Error> {
    if path != ":memory:"
        && let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        // Use ensure_private_dir (mode 0o700) so SQLite WAL/SHM files —
        // which inherit umask defaults and may contain credential
        // snapshots from job payloads — are not group/world-readable
        // on multi-user hosts.
        //
        if let Err(e) = crate::paths::ensure_private_dir_async(parent.to_path_buf()).await {
            return Err(sqlx::Error::Configuration(
                format!(
                    "sqlite: refusing to open database because its parent directory could not be secured: {e}"
                )
                .into(),
            ));
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
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let db_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| {
                sqlx::Error::Configuration(
                    format!(
                        "sqlite: refusing to open database because it could not be secured: {e}"
                    )
                    .into(),
                )
            })?;
        let metadata = db_file.metadata().map_err(|e| {
            sqlx::Error::Configuration(
                format!(
                    "sqlite: refusing to open database because metadata verification failed: {e}"
                )
                .into(),
            )
        })?;
        if !metadata.is_file() {
            return Err(sqlx::Error::Configuration(
                "sqlite: refusing to open a non-regular database target".into(),
            ));
        }
        db_file
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| {
                sqlx::Error::Configuration(
                    format!(
                        "sqlite: refusing to open database because mode 0600 could not be enforced: {e}"
                    )
                    .into(),
                )
            })?;
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

    let persist_wal = path != ":memory:";
    SqlitePoolOptions::new()
        .max_connections(DEFAULT_SQLITE_POOL_CONNECTIONS)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                if persist_wal {
                    enable_persistent_wal(conn).await?;
                }
                Ok(())
            })
        })
        .after_release(|conn, _meta| Box::pin(rollback_on_release(conn)))
        .connect_with(opts)
        .await
}

fn preserve_corrupted_database(path: &Path) -> Result<(), sqlx::Error> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sources = [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ];
    let existing = sources
        .iter()
        .filter(|source| source.exists())
        .collect::<Vec<_>>();
    let mut preserved = Vec::with_capacity(existing.len());
    for source in &existing {
        if !source.is_file() {
            return Err(sqlite_config_error(format!(
                "sqlite: refusing recovery because corrupt state could not be preserved: {} is not a regular file",
                source.display()
            )));
        }
        let suffix = if source.as_path() == path {
            ""
        } else if source.to_string_lossy().ends_with("-wal") {
            "-wal"
        } else {
            "-shm"
        };
        let destination = PathBuf::from(format!("{}.corrupted.{ts}{suffix}", path.display()));
        std::fs::copy(source, &destination).map_err(|error| {
            sqlite_config_error(format!(
                "sqlite: refusing recovery because corrupt state could not be preserved: {error}"
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| {
                    sqlite_config_error(format!(
                        "sqlite: refusing recovery because preserved state could not be secured: {error}"
                    ))
                })?;
        }
        preserved.push(destination);
    }
    for source in existing {
        std::fs::remove_file(source).map_err(|error| {
            sqlite_config_error(format!(
                "sqlite: corrupt state was preserved but active files could not be removed: {error}"
            ))
        })?;
    }
    tracing::info!(path = %path.display(), files = preserved.len(), "sqlite: preserved corrupt database state");
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn active_db_lock_count_for_tests() -> usize {
    ACTIVE_DB_LOCKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .len()
}

pub fn reset_runtime_health_for_tests() {
    *SQLITE_RUNTIME_HEALTH
        .get_or_init(|| Mutex::new(SqliteRuntimeHealth::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = SqliteRuntimeHealth::default();
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;

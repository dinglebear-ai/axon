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
    rename_corrupted(path);
    Ok(())
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

fn rename_corrupted(path: &Path) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dest = PathBuf::from(format!("{}.corrupted.{ts}", path.display()));
    if let Err(e) = std::fs::rename(path, &dest) {
        tracing::warn!(src = %path.display(), dst = %dest.display(), error = %e, "sqlite: could not rename corrupted db");
    } else {
        tracing::info!(src = %path.display(), dst = %dest.display(), "sqlite: renamed corrupted db");
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
        let _ = std::fs::remove_file(&sidecar);
    }
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

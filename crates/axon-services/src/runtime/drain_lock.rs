//! Cross-process worker liveness lock.
//!
//! Every worker-bearing runtime — `axon jobs worker` (including the drainer the
//! CLI auto-spawns after a detached source enqueue) *and* the long-lived
//! `axon serve` / HTTP `axon mcp` schedulers context — holds this lock for its
//! lifetime, so a concurrent CLI invocation can tell "a worker process is
//! already draining this queue" apart from "nobody will pick this job up". The
//! lock is a dedicated zero-table SQLite database held under `BEGIN EXCLUSIVE`:
//! the kernel drops the underlying file lock when the holding process exits
//! (including SIGKILL), so there is no stale-pidfile or TTL-lease failure mode,
//! and the same code path works on Linux, macOS, and Windows.
//!
//! The lock file is a **sibling of the jobs database** (`<jobs.db>.drain-lock`),
//! not the data dir, so its identity tracks the queue identity even when
//! `AXON_SQLITE_PATH` redirects the jobs DB away from the default data dir.
//!
//! The lock is advisory for *spawn dedup only* — job execution correctness
//! never depends on it. Workers claim jobs transactionally from the unified
//! store, so two racing drainers (or a drainer racing `axon serve`) merely
//! split the queue between them.
//!
//! ## Holder vs probe asymmetry
//!
//! [`WorkerDrainLock::try_hold`] (the durable holder) connects with a
//! [`HOLD_BUSY_TIMEOUT`] so its `BEGIN EXCLUSIVE` *waits out* a transient probe
//! rather than losing the lock to it — this is what stops [`is_held`] from
//! evicting a just-starting worker. [`is_held`] (the enqueue-side probe)
//! connects with a zero busy-timeout so it never blocks the enqueue path: if a
//! holder is present it returns `true` immediately.
//!
//! ## Squat property
//!
//! Any local process holding `BEGIN EXCLUSIVE` on the lock file (even an idle
//! `sqlite3 <path> "BEGIN EXCLUSIVE;"` shell) suppresses auto-spawn for its
//! lifetime. This is an accepted same-user-only tradeoff — there is
//! deliberately no TTL/heartbeat staleness path — because job-claim correctness
//! is independent of the lock; only auto-spawn availability is affected.

use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::ConnectOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqliteJournalMode};

pub type DrainLockResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Suffix appended to the jobs-DB path to form the lock-file path.
pub const DRAIN_LOCK_SUFFIX: &str = ".drain-lock";

/// How long a durable holder's `BEGIN EXCLUSIVE` waits for a contended lock
/// before giving up. Long enough to outlast a probe's sub-millisecond hold, so
/// a concurrent [`is_held`] probe cannot evict a starting worker.
const HOLD_BUSY_TIMEOUT: Duration = Duration::from_millis(1000);

/// Resolve the lock-file path for a jobs-database path. The lock lives beside
/// the jobs DB so its identity follows the queue even under `AXON_SQLITE_PATH`.
pub fn drain_lock_path(sqlite_path: &Path) -> PathBuf {
    let mut raw: OsString = sqlite_path.as_os_str().to_owned();
    raw.push(DRAIN_LOCK_SUFFIX);
    PathBuf::from(raw)
}

/// Held for the lifetime of a worker-bearing process. Dropping (or process
/// exit) releases the exclusive lock.
pub struct WorkerDrainLock {
    // Keeping the connection alive keeps the EXCLUSIVE transaction (and the
    // OS-level file lock backing it) held.
    _conn: SqliteConnection,
}

impl WorkerDrainLock {
    /// Try to become the worker-lock holder for `path`.
    ///
    /// Uses [`HOLD_BUSY_TIMEOUT`] so a transient probe cannot evict the caller.
    /// Returns `Ok(None)` when another live process already holds the lock.
    pub async fn try_hold(path: &Path) -> DrainLockResult<Option<Self>> {
        Ok(acquire(path, HOLD_BUSY_TIMEOUT)
            .await?
            .map(|conn| Self { _conn: conn }))
    }

    /// Probe whether some other process currently holds the lock, without
    /// keeping it and without blocking. Used by enqueue paths to skip a
    /// redundant worker spawn.
    pub async fn is_held(path: &Path) -> DrainLockResult<bool> {
        // Zero busy-timeout: a present holder makes this return immediately.
        match acquire(path, Duration::from_millis(0)).await? {
            // We acquired it — nobody was holding it. Drop to release.
            Some(conn) => {
                drop(conn);
                Ok(false)
            }
            None => Ok(true),
        }
    }
}

/// Connect to the lock DB with `busy_timeout` and attempt `BEGIN EXCLUSIVE`.
/// Returns `Some(conn)` with the exclusive transaction open (caller keeps the
/// connection alive to hold the lock), or `None` if another holder has it.
async fn acquire(path: &Path, busy_timeout: Duration) -> DrainLockResult<Option<SqliteConnection>> {
    let mut conn = match connect(path, busy_timeout).await? {
        Some(conn) => conn,
        None => return Ok(None),
    };
    match sqlx::query("BEGIN EXCLUSIVE").execute(&mut conn).await {
        Ok(_) => Ok(Some(conn)),
        Err(err) if is_busy(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

async fn connect(path: &Path, busy_timeout: Duration) -> DrainLockResult<Option<SqliteConnection>> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // sqlx defaults new SQLite connections to WAL, where a read-only
        // `BEGIN EXCLUSIVE` does NOT take a cross-process exclusive OS lock —
        // multiple processes would all "acquire" it and dedup would break.
        // Rollback-journal (DELETE) mode makes `BEGIN EXCLUSIVE` grab the
        // exclusive file lock immediately and hold it for the transaction's
        // lifetime, giving true cross-process mutual exclusion. (Do NOT add
        // `locking_mode = EXCLUSIVE` — it changes same-process lock bookkeeping
        // and breaks in-process exclusion.)
        .journal_mode(SqliteJournalMode::Delete)
        .busy_timeout(busy_timeout)
        .disable_statement_logging();
    match options.connect().await {
        Ok(conn) => Ok(Some(conn)),
        // Another holder can make even connection-time pragmas return BUSY;
        // treat that the same as failing to take the exclusive lock.
        Err(err) if is_busy(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// True for the SQLite `SQLITE_BUSY` (5) / `SQLITE_LOCKED` (6) result codes,
/// matched on the structured driver error code rather than human-readable
/// message text (which varies across SQLite/sqlx versions and locales).
fn is_busy(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if matches!(db.code().as_deref(), Some("5") | Some("6"))
    )
}

#[cfg(test)]
#[path = "drain_lock_tests.rs"]
mod tests;

//! RAII guard for SQLite `BEGIN IMMEDIATE` write transactions.
//!
//! Every job-table write path needs a `BEGIN IMMEDIATE` transaction so
//! concurrent enqueues and claims serialize on SQLite's RESERVED write lock
//! under WAL. sqlx's own `Transaction` type only emits a plain (deferred)
//! `BEGIN`, so axon opens the transaction by hand on a pooled connection.
//!
//! Hand-rolled transactions are easy to leak: a connection dropped between
//! `BEGIN IMMEDIATE` and its matching `COMMIT`/`ROLLBACK` returns to the pool
//! STILL IN A TRANSACTION and poisons that slot — the next checkout's
//! `BEGIN IMMEDIATE` fails ("cannot start a transaction within a transaction"),
//! and enough poisoned slots starve `pool.acquire()` until workers silently
//! stop claiming jobs (a confirmed production incident — see
//! [`crate::store::rollback_on_release`]).
//!
//! [`ImmediateTx`] makes that leak structurally impossible. Callers can only
//! touch the connection through the guard, and the guard guarantees the
//! transaction is resolved one of two ways:
//!
//! - **Explicitly** via [`ImmediateTx::commit`], [`ImmediateTx::rollback`], or
//!   [`ImmediateTx::finish`], each of which consumes the guard and settles the
//!   transaction eagerly so the slot returns to the pool clean immediately.
//! - **Implicitly** when the guard is dropped on an early `?` return or a panic
//!   without being settled: the pool's `after_release` `ROLLBACK` hook scrubs
//!   the dangling transaction when the connection returns to the idle queue.
//!
//! Either way the slot is never left poisoned. This collapses the four
//! previously-duplicated `BEGIN IMMEDIATE` + `commit_or_rollback` /
//! `rollback_best_effort` implementations (`store.rs`, `ops/enqueue.rs`,
//! `ops/lifecycle.rs`, `query.rs`) into one type.

use std::time::{Duration, Instant};

use sqlx::pool::PoolConnection;
use sqlx::{Sqlite, SqliteConnection, SqlitePool};

/// A slow `pool.acquire()` is the canary for SQLite connection-pool starvation
/// (e.g. slots poisoned by leaked transactions, or every connection held by a
/// long write). Anything below this is silent.
const ACQUIRE_WARN_THRESHOLD: Duration = Duration::from_secs(1);

/// An open `BEGIN IMMEDIATE` transaction on a pooled SQLite connection.
///
/// Resolve it with [`commit`](Self::commit), [`rollback`](Self::rollback), or
/// [`finish`](Self::finish). Dropping the guard without resolving it is safe —
/// the pool's `after_release` hook rolls the transaction back — but the eager
/// methods return the slot clean immediately and are always preferred.
#[must_use = "an ImmediateTx must be settled with commit/rollback/finish (or \
              deliberately dropped to roll back); ignoring it begins a transaction \
              and silently discards its writes"]
pub(crate) struct ImmediateTx {
    conn: PoolConnection<Sqlite>,
    /// `true` once `COMMIT`/`ROLLBACK` has run, so `Drop` stays quiet. An
    /// unsettled drop is the safety-net path and worth a `trace!`.
    settled: bool,
}

impl ImmediateTx {
    /// Acquire a pooled connection and open a `BEGIN IMMEDIATE` write
    /// transaction on it.
    ///
    /// Times the checkout: a `pool.acquire()` that blocks for a noticeable
    /// interval is the leading signal of connection-pool starvation, so it is
    /// logged once here for every transactional path rather than at each call
    /// site.
    pub(crate) async fn begin(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let started = Instant::now();
        let mut conn = pool.acquire().await?;
        let waited = started.elapsed();
        if waited >= ACQUIRE_WARN_THRESHOLD {
            tracing::warn!(
                waited_ms = waited.as_millis() as u64,
                "tx: pool.acquire() blocked >=1s — possible SQLite connection-pool starvation"
            );
        }
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        Ok(Self {
            conn,
            settled: false,
        })
    }

    /// Mutable access to the underlying connection for running statements inside
    /// the transaction.
    ///
    /// Run only DML through this handle — do **not** issue `COMMIT`/`ROLLBACK`/
    /// `BEGIN`/`SAVEPOINT`. Settle the transaction via [`commit`](Self::commit),
    /// [`rollback`](Self::rollback), or [`finish`](Self::finish) instead, so the
    /// guard's `settled` bookkeeping and the `Drop` safety net stay accurate.
    pub(crate) fn conn(&mut self) -> &mut SqliteConnection {
        &mut self.conn
    }

    /// Commit the transaction. On commit failure, eagerly `ROLLBACK` so the
    /// connection returns to the pool clean, then surface the original error.
    pub(crate) async fn commit(mut self) -> Result<(), sqlx::Error> {
        self.settled = true;
        match sqlx::query("COMMIT").execute(&mut *self.conn).await {
            Ok(_) => Ok(()),
            Err(commit_err) => {
                if let Err(rollback_err) = sqlx::query("ROLLBACK").execute(&mut *self.conn).await {
                    tracing::warn!(error = %rollback_err, "tx: ROLLBACK after failed COMMIT errored");
                }
                Err(commit_err)
            }
        }
    }

    /// Eagerly `ROLLBACK` (best-effort). Use on early returns that abort the
    /// transaction so the slot returns clean immediately instead of waiting for
    /// the `after_release` hook. A "no transaction is active" error is the
    /// expected no-op (the transaction already self-aborted) and is ignored.
    pub(crate) async fn rollback(mut self) {
        self.settled = true;
        match sqlx::query("ROLLBACK").execute(&mut *self.conn).await {
            Ok(_) => {}
            // A self-aborted transaction (e.g. SQLite rolled it back on a
            // constraint error) leaves nothing to roll back — the expected
            // no-op, not a failure. Mirrors `store::rollback_on_release`.
            Err(sqlx::Error::Database(db)) if db.message().contains("no transaction is active") => {
            }
            Err(e) => tracing::warn!(error = %e, "tx: ROLLBACK errored"),
        }
    }

    /// Settle the transaction from a `Result`: commit on `Ok`, roll back on
    /// `Err`. A commit failure converts the `Ok` into the commit error. Replaces
    /// the old per-file `commit_or_rollback(conn, result)` helpers.
    pub(crate) async fn finish<T, E>(self, result: Result<T, E>) -> Result<T, E>
    where
        E: From<sqlx::Error>,
    {
        match result {
            Ok(value) => {
                self.commit().await?;
                Ok(value)
            }
            Err(err) => {
                self.rollback().await;
                Err(err)
            }
        }
    }
}

impl Drop for ImmediateTx {
    fn drop(&mut self) {
        if !self.settled {
            // The connection still holds an open BEGIN IMMEDIATE. Drop cannot
            // run async SQL, so the pool's `after_release` ROLLBACK hook
            // (`store::rollback_on_release`) scrubs the dangling transaction
            // when this connection returns to the idle queue. This is the safety
            // net that makes early `?` returns and panics leak-free.
            tracing::trace!(
                "tx: guard dropped unsettled; after_release will scrub the transaction"
            );
        }
    }
}

#[cfg(test)]
#[path = "tx_tests.rs"]
mod tests;

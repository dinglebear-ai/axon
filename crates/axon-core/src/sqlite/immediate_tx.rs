//! RAII guard for SQLite `BEGIN IMMEDIATE` write transactions.

use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

const ACQUIRE_WARN_THRESHOLD: Duration = Duration::from_secs(1);

#[must_use = "settle ImmediateTx with commit, rollback, or finish"]
pub struct ImmediateTx {
    tx: Transaction<'static, Sqlite>,
}

impl ImmediateTx {
    pub async fn begin(pool: &SqlitePool) -> Result<Self, sqlx::Error> {
        let started = Instant::now();
        let conn = pool.acquire().await?;
        let waited = started.elapsed();
        if waited >= ACQUIRE_WARN_THRESHOLD {
            tracing::warn!(
                waited_ms = waited.as_millis() as u64,
                "sqlite: connection checkout blocked"
            );
        }
        let tx = Transaction::begin(conn, Some(Cow::Borrowed("BEGIN IMMEDIATE"))).await?;
        Ok(Self { tx })
    }

    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.tx.commit().await
    }

    pub async fn rollback(self) {
        if let Err(error) = self.tx.rollback().await {
            tracing::warn!(error = %error, "sqlite: rollback errored");
        }
    }

    pub async fn finish<T, E>(self, result: Result<T, E>) -> Result<T, E>
    where
        E: From<sqlx::Error>,
    {
        match result {
            Ok(value) => {
                self.commit().await?;
                Ok(value)
            }
            Err(error) => {
                self.rollback().await;
                Err(error)
            }
        }
    }
}

impl Deref for ImmediateTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

impl DerefMut for ImmediateTx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx
    }
}

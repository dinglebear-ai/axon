mod allowed_users;
mod browser;
mod connection;
mod oauth;
mod rows;
mod schema;
mod upstream;

use connection::{open_connections, validate_or_reopen_connection};
use rows::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params};
use tracing::warn;

use crate::error::AuthError;
use crate::types::{
    AllowedUserRow, AuthorizationCodeRow, AuthorizationRequestRow, BrowserLoginStateRow,
    BrowserSessionRow, NativeAuthorizationResultRow, RefreshTokenRow, RegisteredClient,
    UpstreamOauthCredentialRow, UpstreamOauthStateRow,
};

const UPSTREAM_OAUTH_STATE_MAX_TTL_SECS: i64 = 600;
use crate::util::{
    ensure_restrictive_permissions, fingerprint, now_unix, set_restrictive_permissions,
};

const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const SQLITE_POOL_SIZE: usize = 4;

#[derive(Clone, Debug)]
pub struct SqliteStore {
    conns: Arc<Vec<Mutex<Connection>>>,
    next_conn: Arc<AtomicUsize>,
    path: Arc<PathBuf>,
}

impl SqliteStore {
    pub async fn open(path: PathBuf) -> Result<Self, AuthError> {
        let path_for_open = path.clone();
        let conns = tokio::task::spawn_blocking(move || {
            open_connections(path_for_open.as_path(), SQLITE_POOL_SIZE)
        })
        .await;
        let store = match conns {
            Ok(result) => result,
            Err(error) => Err(AuthError::Storage(format!(
                "sqlite open task failed: {error}"
            ))),
        }
        .map(|conns| Self {
            conns: Arc::new(conns.into_iter().map(Mutex::new).collect()),
            next_conn: Arc::new(AtomicUsize::new(0)),
            path: Arc::new(path),
        })?;

        store.cleanup_expired().await?;
        Ok(store)
    }

    pub async fn pragma(&self, name: &str) -> Result<String, AuthError> {
        let pragma = match name {
            "journal_mode" | "busy_timeout" | "foreign_keys" => name.to_string(),
            other => {
                return Err(AuthError::Config(format!(
                    "unsupported pragma query `{other}`"
                )));
            }
        };

        self.with_conn(move |conn| {
            conn.query_row(&format!("PRAGMA {pragma};"), [], |row| {
                row.get::<_, Value>(0)
            })
            .map(|value| match value {
                Value::Text(text) => text,
                Value::Integer(int) => int.to_string(),
                other => format!("{other:?}"),
            })
            .map_err(sqlite_error)
        })
        .await
    }

    /// Delete expired rows from all short-lived tables. Also drops upstream OAuth
    /// credential rows whose access token has expired AND have no refresh token
    /// available for re-use (SEC-9). Returns the total number of deleted rows.
    pub async fn cleanup_expired(&self) -> Result<u64, AuthError> {
        let now = now_unix();
        self.with_conn(move |conn| {
            let mut total: u64 = 0;
            for table in [
                "authorization_requests",
                "authorization_codes",
                "refresh_tokens",
                "browser_sessions",
                "browser_login_states",
                "native_authorization_results",
            ] {
                let deleted = conn
                    .execute(
                        &format!("DELETE FROM {table} WHERE expires_at <= ?1"),
                        params![now],
                    )
                    .map_err(sqlite_error)?;
                total += deleted as u64;
            }
            let deleted = conn
                .execute(
                    "DELETE FROM upstream_oauth_state WHERE expires_at <= ?1",
                    params![now],
                )
                .map_err(sqlite_error)?;
            total += deleted as u64;
            let deleted = conn
                .execute(
                    "DELETE FROM upstream_oauth_credentials
                     WHERE access_token_expires_at <= ?1 AND refresh_token_present = 0",
                    params![now],
                )
                .map_err(sqlite_error)?;
            total += deleted as u64;
            Ok(total)
        })
        .await
    }

    async fn with_conn<T, F>(&self, op: F) -> Result<T, AuthError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, AuthError> + Send + 'static,
    {
        let conns = Arc::clone(&self.conns);
        let path = Arc::clone(&self.path);
        let len = conns.len();
        let idx = self.next_conn.fetch_add(1, Ordering::Relaxed) % len;
        tokio::task::spawn_blocking(move || {
            let mut guard = conns[idx]
                .lock()
                .map_err(|_| AuthError::Storage("sqlite mutex poisoned".to_string()))?;
            validate_or_reopen_connection(&mut guard, path.as_ref())?;
            op(&guard)
        })
        .await
        .map_err(|error| AuthError::Storage(format!("sqlite task failed: {error}")))?
    }

    #[cfg(test)]
    fn connection_count(&self) -> usize {
        self.conns.len()
    }
}

#[allow(clippy::needless_pass_by_value)]
fn sqlite_error(error: rusqlite::Error) -> AuthError {
    AuthError::Storage(format!("sqlite error: {error}"))
}

#[cfg(test)]
#[path = "sqlite/tests.rs"]
mod tests;

use super::*;

pub(super) fn open_connections(path: &Path, count: usize) -> Result<Vec<Connection>, AuthError> {
    (0..count).map(|_| open_connection(path)).collect()
}

#[allow(clippy::too_many_lines)]
fn open_connection(path: &Path) -> Result<Connection, AuthError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            AuthError::Storage(format!(
                "create auth database directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    let existed = path.exists();
    if existed {
        ensure_restrictive_permissions(path)?;
    }

    let conn = Connection::open(path).map_err(sqlite_error)?;
    conn.busy_timeout(std::time::Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(sqlite_error)?;
    conn.execute_batch(schema::SCHEMA_SQL)
        .map_err(sqlite_error)?;

    if !existed {
        set_restrictive_permissions(path)?;
    }
    ensure_restrictive_permissions(path)?;

    Ok(conn)
}

pub(super) fn validate_or_reopen_connection(
    conn: &mut Connection,
    path: &Path,
) -> Result<(), AuthError> {
    let Err(error) = conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)) else {
        return Ok(());
    };
    warn!(
        path = %path.display(),
        error = %error,
        "stale sqlite connection detected, reopening"
    );

    *conn = open_connection(path)?;
    conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
        .map(|_| ())
        .map_err(sqlite_error)
}

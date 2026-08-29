use rusqlite::Connection;

use super::db_error;

pub(super) fn migrate_columns(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(codex_control_operations)")
        .map_err(db_error)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(db_error)?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(db_error)?;
    drop(statement);
    for (name, statement) in [
        (
            "approved_at",
            "ALTER TABLE codex_control_operations ADD COLUMN approved_at INTEGER",
        ),
        (
            "expires_at",
            "ALTER TABLE codex_control_operations ADD COLUMN expires_at INTEGER",
        ),
        (
            "redacted_request",
            "ALTER TABLE codex_control_operations ADD COLUMN redacted_request TEXT NOT NULL DEFAULT '{}'",
        ),
        (
            "execution_attempt_id",
            "ALTER TABLE codex_control_operations ADD COLUMN execution_attempt_id TEXT",
        ),
        (
            "response_evidence",
            "ALTER TABLE codex_control_operations ADD COLUMN response_evidence TEXT",
        ),
    ] {
        if !columns.contains(name) {
            connection.execute(statement, []).map_err(db_error)?;
        }
    }
    Ok(())
}

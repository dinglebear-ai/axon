//! In-crate SQLite schema for the durable memory store.
//!
//! Four tables back the store, matching the memory-contract data model:
//! - `memory_records`   — one row per memory (type/status/body/scope/decay/history)
//! - `memory_links`     — evidence-backed links from a memory to sources/entities
//! - `memory_reinforcement` — append-only positive-use signals (reinforce log)
//! - `memory_reviews`   — review-queue entries with reason + timestamps
//!
//! `ensure_schema` is idempotent (`CREATE TABLE IF NOT EXISTS`) so it is safe to
//! call on every store construction.

use rusqlite::Connection;

/// Schema version stamped into `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS memory_records (
    memory_id           TEXT PRIMARY KEY,
    memory_type         TEXT NOT NULL,
    status              TEXT NOT NULL,
    body                TEXT NOT NULL,
    title               TEXT,
    confidence          REAL NOT NULL,
    salience            REAL NOT NULL,
    scope_kind          TEXT NOT NULL,
    scope_value         TEXT NOT NULL,
    decay_json          TEXT,
    history_json        TEXT NOT NULL DEFAULT '[]',
    embedding_refs_json TEXT NOT NULL DEFAULT '[]',
    superseded_by       TEXT,
    contradicts         TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_records_status
    ON memory_records(status);
CREATE INDEX IF NOT EXISTS idx_memory_records_scope
    ON memory_records(scope_kind, scope_value);
CREATE INDEX IF NOT EXISTS idx_memory_records_type
    ON memory_records(memory_type);

CREATE TABLE IF NOT EXISTS memory_links (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id  TEXT NOT NULL,
    link_type  TEXT NOT NULL,
    target     TEXT NOT NULL,
    confidence REAL NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memory_records(memory_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memory_links_memory
    ON memory_links(memory_id);

CREATE TABLE IF NOT EXISTS memory_reinforcement (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id  TEXT NOT NULL,
    amount     REAL NOT NULL,
    reason     TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memory_records(memory_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memory_reinforcement_memory
    ON memory_reinforcement(memory_id);

CREATE TABLE IF NOT EXISTS memory_reviews (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id   TEXT NOT NULL,
    reason      TEXT,
    resolved    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    resolved_at TEXT,
    FOREIGN KEY (memory_id) REFERENCES memory_records(memory_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memory_reviews_open
    ON memory_reviews(resolved);
"#;

/// Create all memory tables and indices if they do not exist, enable foreign
/// keys, and stamp the schema version.
pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;

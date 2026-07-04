use super::*;
use rusqlite::Connection;

fn tables(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

#[test]
fn ensure_schema_creates_all_four_tables() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_schema(&conn).unwrap();
    let names = tables(&conn);
    for expected in [
        "memory_links",
        "memory_records",
        "memory_reinforcement",
        "memory_reviews",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
}

#[test]
fn ensure_schema_is_idempotent() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_schema(&conn).unwrap();
    // Second call must not error (CREATE TABLE IF NOT EXISTS).
    ensure_schema(&conn).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn foreign_keys_are_enabled() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_schema(&conn).unwrap();
    let fk: i64 = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

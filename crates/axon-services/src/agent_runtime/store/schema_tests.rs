use super::*;

#[test]
fn partially_migrated_schema_adds_only_missing_columns() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    conn.pragma_update(None, "user_version", 0).unwrap();

    migrate(&conn).unwrap();

    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    verify_columns(&conn).unwrap();
}

#[test]
fn migration_does_not_advance_version_when_schema_creation_fails() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE VIEW agent_turns AS SELECT 1 AS id;")
        .unwrap();

    assert!(migrate(&conn).is_err());
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

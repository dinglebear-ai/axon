//! Indexed outbox inventory regression coverage.
use super::*;

fn query_plan(db: &Connection) -> String {
    let mut statement = db.prepare(
        "EXPLAIN QUERY PLAN SELECT name FROM reservations WHERE substr(name,-5)='.json' AND name>?1 ORDER BY name LIMIT ?2",
    ).unwrap();
    statement
        .query_map(params!["", 1025_i64], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n")
}

#[test]
fn inventory_uses_partial_index_and_upgrades_existing_database_without_it() {
    let temp = tempfile::tempdir().unwrap();
    let db = open(temp.path(), true).unwrap();
    assert!(query_plan(&db).contains("reservations_pending_names"));
    db.execute_batch("DROP INDEX reservations_pending_names;")
        .unwrap();
    drop(db);
    let db = open(temp.path(), false).unwrap();
    let plan = query_plan(&db);
    assert!(plan.contains("reservations_pending_names"), "{plan}");
    assert!(!plan.contains("TEMP B-TREE"), "{plan}");
}

#[test]
fn two_thousand_retained_pages_are_monotonic_exhaustive_and_do_not_reconcile_again() {
    let temp = tempfile::tempdir().unwrap();
    RECONCILIATIONS.with(|count| count.set(0));
    let mut db = open(temp.path(), true).unwrap();
    let expected = (0..2049)
        .map(|n| format!("delivery-{n:05}.json"))
        .collect::<Vec<_>>();
    {
        let tx = db.transaction().unwrap();
        for name in &expected {
            tx.execute("INSERT INTO reservations VALUES(?,1)", [name])
                .unwrap();
        }
        // These remain charged but must never consume pending-delivery pages.
        for n in 0..2049 {
            tx.execute(
                "INSERT INTO reservations VALUES(?,1)",
                [format!("delivery-{n:05}.invalid")],
            )
            .unwrap();
        }
        tx.execute("UPDATE usage SET bytes=4098 WHERE singleton=1", [])
            .unwrap();
        tx.commit().unwrap();
    }
    let mut cursor = String::new();
    let mut observed = Vec::new();
    for index in 0..expected.len() {
        let (page, has_more) = inventory_page(&db, &cursor, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert!(page[0] > cursor);
        assert_eq!(has_more, index + 1 < expected.len());
        cursor = page[0].clone();
        observed.extend(page);
    }
    assert_eq!(observed, expected);
    assert_eq!(
        inventory_page(&db, &cursor, 1).unwrap(),
        (Vec::new(), false)
    );
    assert_eq!(RECONCILIATIONS.with(|count| count.get()), 1);
    let charged: u64 = db
        .query_row("SELECT bytes FROM usage", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        charged, 4098,
        "enumerating does not release quarantine charges"
    );
}

#[test]
fn inventory_keeps_quarantine_charge_and_finds_same_key_retry() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("delivery.json");
    let quarantine = temp.path().join("delivery.invalid");
    std::fs::write(&path, b"123456").unwrap();
    drop(open(temp.path(), true).unwrap());
    quarantine_file(&path, &quarantine, 12).unwrap();
    let mut db = open(temp.path(), false).unwrap();
    assert_eq!(inventory_page(&db, "", 1).unwrap(), (Vec::new(), false));
    reserve(&mut db, &path, 6, 12).unwrap();
    std::fs::write(&path, b"abcdef").unwrap();
    assert_eq!(
        inventory_page(&db, "", 1).unwrap(),
        (vec!["delivery.json".to_owned()], false)
    );
    assert!(reserve(&mut db, &temp.path().join("overflow.json"), 1, 12).is_err());
    assert!(quarantine.exists());
}

#[test]
fn inventory_is_bounded_to_page_plus_lookahead_and_rejects_unsafe_names() {
    let temp = tempfile::tempdir().unwrap();
    let db = open(temp.path(), true).unwrap();
    for name in ["a.json", "b.json", "z/escape.json"] {
        db.execute("INSERT INTO reservations VALUES(?,1)", [name])
            .unwrap();
    }
    // The unsafe third row cannot be read by a one-result + one-lookahead query.
    assert_eq!(
        inventory_page(&db, "", 1).unwrap(),
        (vec!["a.json".to_owned()], true)
    );
    assert!(inventory_page(&db, "b.json", 1).is_err());
    assert!(inventory_page(&db, "", 0).is_err());
    assert!(inventory_page(&db, "", super::super::MAX_PENDING_DELIVERIES + 1).is_err());
}

#[test]
fn legacy_files_are_discovered_on_initial_inventory_open() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["b.json", "a.json", "retained.invalid"] {
        std::fs::write(temp.path().join(name), b"legacy").unwrap();
    }
    let db = open(temp.path(), true).unwrap();
    assert_eq!(
        inventory_page(&db, "", 1024).unwrap(),
        (vec!["a.json".to_owned(), "b.json".to_owned()], false)
    );
    let charged: u64 = db
        .query_row("SELECT bytes FROM usage", [], |row| row.get(0))
        .unwrap();
    assert_eq!(charged, 18);
}

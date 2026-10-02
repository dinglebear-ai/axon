use super::*;

#[test]
fn physical_stage_budget_allows_sqlite_overhead_and_counts_all_sidecars() {
    let directory =
        std::env::temp_dir().join(format!("axon-stage-size-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("stage.sqlite");
    let database = fs::File::create(&path).unwrap();
    let physical_limit = 4 * LIMIT;
    // Sparse files test the byte guard without writing or allocating 1 GiB.
    database.set_len(LIMIT + 1).unwrap();
    check_size(&path).expect("SQLite overhead may exceed the logical payload budget");
    database.set_len(physical_limit / 2).unwrap();
    let wal = fs::File::create(path.with_extension("sqlite-wal")).unwrap();
    wal.set_len(physical_limit / 2 - 2).unwrap();
    let shm = fs::File::create(path.with_extension("sqlite-shm")).unwrap();
    shm.set_len(1).unwrap();
    let journal = fs::File::create(path.with_extension("sqlite-journal")).unwrap();
    journal.set_len(1).unwrap();
    check_size(&path).expect("the exact combined physical limit is allowed");
    journal.set_len(2).unwrap();
    let error = check_size(&path).expect_err("all sidecars count toward the physical limit");
    assert_eq!(
        error.details.get("observed_bytes"),
        Some(&(physical_limit + 1).to_string())
    );
    assert_eq!(
        error.details.get("limit_bytes"),
        Some(&physical_limit.to_string())
    );
    drop((database, wal, shm, journal));
    fs::remove_dir_all(directory).unwrap();
}

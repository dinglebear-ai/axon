use super::*;

fn encoded(key: &str, value: u64) -> Vec<u8> {
    serde_json::to_vec(&(key, value)).unwrap()
}

#[test]
fn disk_keys_reopen_in_order_and_sealed_records_are_immutable() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("records.sqlite");
    let mut store = RecordStore::create(&path, "g").unwrap();
    for index in 0..1000 {
        let key = format!("side-effects:{index}");
        assert!(store.append(&key, &encoded(&key, index)).unwrap());
    }
    assert!(
        !store
            .append("side-effects:0", &encoded("side-effects:0", 999))
            .unwrap()
    );
    let seal = store.seal().unwrap();
    assert_eq!(seal.records, 1000);
    assert_eq!(store.seal().unwrap(), seal);
    assert!(
        store
            .append("after-seal", &encoded("after-seal", 1))
            .is_err()
    );
    let cache: i64 = store
        .db
        .pragma_query_value(None, "cache_size", |row| row.get(0))
        .unwrap();
    let pages: i64 = store
        .db
        .pragma_query_value(None, "max_page_count", |row| row.get(0))
        .unwrap();
    assert_eq!(cache, -4096);
    assert_eq!(pages, 65536);
    drop(store);
    assert!(RecordStore::open(&path, "other-generation").is_err());
    let store = RecordStore::open(&path, "g").unwrap();
    let mut visited = 0;
    let actual = store
        .visit_raw(|key, payload, sequence| {
            assert_eq!(key, format!("side-effects:{visited}"));
            assert_eq!(payload, encoded(key, visited));
            assert_eq!(sequence, visited + 1);
            visited += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(actual, seal);
}

#[test]
fn ownership_lock_and_private_paths_fence_recovery() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("records.sqlite");
    let store = RecordStore::create(&path, "g").unwrap();
    assert!(
        RecordStore::open(&path, "g")
            .err()
            .unwrap()
            .to_string()
            .contains("owner")
    );
    drop(store);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let alias = directory.path().join("symlink.sqlite");
        symlink(&path, &alias).unwrap();
        assert!(RecordStore::open(&alias, "g").is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(RecordStore::open(&path, "g").is_err());
    }
}

#[test]
fn disk_full_append_rolls_back_and_keeps_prior_records_recoverable() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("full.sqlite");
    let mut store = RecordStore::create(&path, "g").unwrap();
    store.append("good", &encoded("good", 1)).unwrap();
    let pages: u64 = store
        .db
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    store
        .db
        .pragma_update(None, "max_page_count", pages + 2)
        .unwrap();
    let error = store
        .append("too-large-for-disk", &vec![b'x'; 64 * 1024])
        .unwrap_err();
    assert!(error.to_string().contains("full"), "{error}");
    assert!(!store.contains("too-large-for-disk").unwrap());
    assert_eq!(store.seal().unwrap().records, 1);
    drop(store);
    assert_eq!(
        RecordStore::open(&path, "g")
            .unwrap()
            .sealed()
            .unwrap()
            .unwrap()
            .records,
        1
    );
}

#[test]
fn oversized_corrupt_payload_is_rejected_from_lengths_before_loading_blob() {
    let directory = super::super::private_tempdir().unwrap();
    let store = RecordStore::create(&directory.path().join("oversize.sqlite"), "g").unwrap();
    store
        .db
        .execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    store
        .db
        .execute(
            "INSERT INTO spool_records(record_key,payload,digest) VALUES('oversize',zeroblob(?),?)",
            params![MAX_WINDOW_BYTES + 1, "0".repeat(64)],
        )
        .unwrap();
    let mut delivered = false;
    let error = store
        .visit_raw(|_, _, _| {
            delivered = true;
            Ok(())
        })
        .unwrap_err();
    assert!(!delivered);
    assert!(error.to_string().contains("byte window"));
}

#[test]
fn corruption_and_missing_sealed_records_fail_closed() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("seal.sqlite");
    let mut store = RecordStore::create(&path, "g").unwrap();
    store.append("good", &encoded("good", 1)).unwrap();
    store.seal().unwrap();
    store
        .db
        .execute("UPDATE spool_records SET payload=?", [encoded("good", 2)])
        .unwrap();
    assert!(
        store
            .visit_raw(|_, _, _| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("digest mismatch")
    );
    store.db.execute("DELETE FROM spool_records", []).unwrap();
    drop(store);
    assert!(
        RecordStore::open(&path, "g")
            .err()
            .unwrap()
            .to_string()
            .contains("seal differs")
    );
}

#[test]
fn child_crash_preserves_committed_record_and_rolls_back_uncommitted_record() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("crash.sqlite");
    let mut store = RecordStore::create(&path, "crash-generation").unwrap();
    store.append("committed", &encoded("committed", 1)).unwrap();
    drop(store);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "source::executor::generation_spool::storage::tests::crash_writer_child",
            "--nocapture",
        ])
        .env("TEST_GENERATION_SPOOL_CRASH_PATH", &path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(37),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut recovered = RecordStore::open(&path, "crash-generation").unwrap();
    assert!(recovered.contains("committed").unwrap());
    assert!(!recovered.contains("uncommitted").unwrap());
    assert_eq!(recovered.seal().unwrap().records, 1);
}

#[test]
fn crash_writer_child() {
    let Some(path) = std::env::var_os("TEST_GENERATION_SPOOL_CRASH_PATH") else {
        return;
    };
    let store = RecordStore::open(Path::new(&path), "crash-generation").unwrap();
    store.db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let payload = encoded("uncommitted", 2);
    store
        .db
        .execute(
            "INSERT INTO spool_records(record_key,payload,digest) VALUES(?,?,?)",
            params![
                "uncommitted",
                &payload,
                hex::encode(Sha256::digest(&payload))
            ],
        )
        .unwrap();
    // Force uncommitted dirty pages to disk so reopening must recover a
    // real rollback journal, rather than merely losing a cached insert.
    store.db.cache_flush().unwrap();
    assert!(!store.db.is_autocommit());
    let mut journal = Path::new(&path).as_os_str().to_os_string();
    journal.push("-journal");
    assert!(std::fs::metadata(Path::new(&journal)).unwrap().len() > 0);
    // Simulate termination without Rust/SQLite destructors settling the tx.
    std::process::exit(37);
}

#[test]
fn unknown_format_and_existing_file_are_never_reinitialized() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("version.sqlite");
    let store = RecordStore::create(&path, "owner").unwrap();
    store.db.pragma_update(None, "user_version", 2).unwrap();
    // Check format validation while this test still owns the store. Recovery
    // can also legitimately fail at the earlier owner-lock boundary.
    let error = store.validate("owner").unwrap_err();
    assert!(error.to_string().contains("unsupported"), "{error:#}");
    drop(store);
    assert!(RecordStore::open(&path, "owner").is_err());
    let legacy = directory.path().join("legacy.sqlite");
    std::fs::write(&legacy, b"legacy data must remain untouched").unwrap();
    assert!(RecordStore::create(&legacy, "owner").is_err());
    assert_eq!(
        std::fs::read(&legacy).unwrap(),
        b"legacy data must remain untouched"
    );
}

#[test]
#[ignore = "explicit local spool memory profile"]
fn spool_memory_profile() {
    let records: u64 = std::env::var("TEST_GENERATION_SPOOL_PROFILE_RECORDS")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=100_000).contains(&records));
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("profile.sqlite");
    let mut store = RecordStore::create(&path, "memory-profile").unwrap();
    let content = "x".repeat(1024);
    for index in 0..records {
        let key = format!("record:{index:08}");
        let payload = super::super::codec::encode(&(&key, &content), MAX_WINDOW_BYTES).unwrap();
        store.append(&key, &payload).unwrap();
    }
    let seal = store.seal().unwrap();
    assert_eq!(seal.records, records);
    println!(
        "SPOOL_PROFILE records={records} payload_bytes={} database_bytes={} cache_target_kib=4096",
        seal.payload_bytes,
        std::fs::metadata(&path).unwrap().len()
    );
}

#[test]
fn dropping_store_releases_owner_even_with_a_duplicated_open_description() {
    let directory = super::super::private_tempdir().unwrap();
    let path = directory.path().join("duplicated-owner.sqlite");
    let store = RecordStore::create(&path, "g").unwrap();
    // try_clone shares the open description, as an inherited descriptor does.
    // This reproduces the lock lifetime without unsafe fork in a Rust harness.
    let inherited = store._owner.0.try_clone().unwrap();
    drop(store);
    let reopened = RecordStore::open(&path, "g");
    drop(inherited);
    reopened
        .expect("an unrelated duplicated descriptor must not retain ownership after SQLite closes");
}

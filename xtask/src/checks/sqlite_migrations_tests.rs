use super::*;
use std::fs;
use tempfile::TempDir;

fn write_fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let migrations_dir = tmp.path().join(MIGRATIONS_DIR);
    fs::create_dir_all(&migrations_dir).unwrap();
    fs::create_dir_all(tmp.path().join("crates/axon-jobs/src")).unwrap();

    fs::write(
        migrations_dir.join("0001_create_tables.sql"),
        "CREATE TABLE jobs (id TEXT PRIMARY KEY);\n",
    )
    .unwrap();
    fs::write(
        migrations_dir.join("0002_add_status.sql"),
        "ALTER TABLE jobs ADD COLUMN status TEXT;\n",
    )
    .unwrap();

    let first = sha384_file(&migrations_dir.join("0001_create_tables.sql")).unwrap();
    let second = sha384_file(&migrations_dir.join("0002_add_status.sql")).unwrap();
    fs::write(
        tmp.path().join(CHECKSUMS_FILE),
        format!("# fixture\n0001_create_tables.sql {first}\n0002_add_status.sql {second}\n"),
    )
    .unwrap();

    tmp
}

#[test]
fn check_accepts_sequential_checksum_pinned_migrations() {
    let tmp = write_fixture();

    assert_eq!(check_inner(tmp.path()).unwrap(), 2);
}

#[test]
fn check_rejects_changed_migration_contents() {
    let tmp = write_fixture();
    fs::write(
        tmp.path()
            .join(MIGRATIONS_DIR)
            .join("0001_create_tables.sql"),
        "CREATE TABLE jobs (id TEXT PRIMARY KEY, mutated TEXT);\n",
    )
    .unwrap();

    let err = check_inner(tmp.path()).unwrap_err().to_string();
    assert!(err.contains("SQLite migration checksum drift"), "{err}");
}

#[test]
fn check_rejects_missing_manifest_entry_for_new_migration() {
    let tmp = write_fixture();
    fs::write(
        tmp.path().join(MIGRATIONS_DIR).join("0003_add_kind.sql"),
        "ALTER TABLE jobs ADD COLUMN kind TEXT;\n",
    )
    .unwrap();

    let err = check_inner(tmp.path()).unwrap_err().to_string();
    assert!(
        err.contains("SQLite migration checksum manifest is out of sync"),
        "{err}"
    );
}

#[test]
fn check_rejects_sequence_gaps() {
    let tmp = write_fixture();
    fs::rename(
        tmp.path().join(MIGRATIONS_DIR).join("0002_add_status.sql"),
        tmp.path().join(MIGRATIONS_DIR).join("0003_add_status.sql"),
    )
    .unwrap();

    let err = check_inner(tmp.path()).unwrap_err().to_string();
    assert!(err.contains("migration sequence gap or reorder"), "{err}");
}

#[test]
fn update_writes_manifest_for_new_migration() {
    let tmp = write_fixture();
    fs::write(
        tmp.path().join(MIGRATIONS_DIR).join("0003_add_kind.sql"),
        "ALTER TABLE jobs ADD COLUMN kind TEXT;\n",
    )
    .unwrap();

    update(tmp.path()).unwrap();

    let manifest = fs::read_to_string(tmp.path().join(CHECKSUMS_FILE)).unwrap();
    assert!(manifest.contains("0001_create_tables.sql"), "{manifest}");
    assert!(manifest.contains("0002_add_status.sql"), "{manifest}");
    assert!(manifest.contains("0003_add_kind.sql"), "{manifest}");
    assert_eq!(check_inner(tmp.path()).unwrap(), 3);
}

#[test]
fn render_manifest_documents_append_only_rule() {
    let manifest = render_manifest(&[MigrationEntry {
        name: "0001_create_tables.sql".to_owned(),
        checksum: "a".repeat(96),
    }]);

    assert!(
        manifest.contains("Migrations are append-only once merged"),
        "{manifest}"
    );
    assert!(manifest.ends_with(&format!("0001_create_tables.sql {}\n", "a".repeat(96))));
}

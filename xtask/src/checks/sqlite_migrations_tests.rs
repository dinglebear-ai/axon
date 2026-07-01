use super::*;
use std::fs;
use tempfile::TempDir;

fn write_fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_manifest_fixture(
        &tmp,
        &MIGRATION_MANIFESTS[0],
        &[
            (
                "0001_create_tables.sql",
                "CREATE TABLE jobs (id TEXT PRIMARY KEY);\n",
            ),
            (
                "0002_add_status.sql",
                "ALTER TABLE jobs ADD COLUMN status TEXT;\n",
            ),
        ],
    );
    write_manifest_fixture(
        &tmp,
        &MIGRATION_MANIFESTS[1],
        &[(
            "0001_create_ledger.sql",
            "CREATE TABLE sources (id TEXT PRIMARY KEY);\n",
        )],
    );

    tmp
}

fn write_manifest_fixture(tmp: &TempDir, manifest: &MigrationManifest, files: &[(&str, &str)]) {
    let migrations_dir = tmp.path().join(manifest.migrations_dir);
    fs::create_dir_all(&migrations_dir).unwrap();
    fs::create_dir_all(tmp.path().join(manifest.checksums_file).parent().unwrap()).unwrap();

    let mut checksum_lines = String::from("# fixture\n");
    for (name, contents) in files {
        fs::write(migrations_dir.join(name), contents).unwrap();
        let checksum = sha384_file(&migrations_dir.join(name)).unwrap();
        checksum_lines.push_str(&format!("{name} {checksum}\n"));
    }
    fs::write(tmp.path().join(manifest.checksums_file), checksum_lines).unwrap();
}

#[test]
fn check_accepts_sequential_checksum_pinned_migrations() {
    let tmp = write_fixture();

    assert_eq!(check_inner(tmp.path()).unwrap(), 3);
}

#[test]
fn check_rejects_changed_migration_contents() {
    let tmp = write_fixture();
    fs::write(
        tmp.path()
            .join(MIGRATION_MANIFESTS[0].migrations_dir)
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
        tmp.path()
            .join(MIGRATION_MANIFESTS[0].migrations_dir)
            .join("0003_add_kind.sql"),
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
        tmp.path()
            .join(MIGRATION_MANIFESTS[0].migrations_dir)
            .join("0002_add_status.sql"),
        tmp.path()
            .join(MIGRATION_MANIFESTS[0].migrations_dir)
            .join("0003_add_status.sql"),
    )
    .unwrap();

    let err = check_inner(tmp.path()).unwrap_err().to_string();
    assert!(err.contains("migration sequence gap or reorder"), "{err}");
}

#[test]
fn update_writes_manifest_for_new_migration() {
    let tmp = write_fixture();
    fs::write(
        tmp.path()
            .join(MIGRATION_MANIFESTS[0].migrations_dir)
            .join("0003_add_kind.sql"),
        "ALTER TABLE jobs ADD COLUMN kind TEXT;\n",
    )
    .unwrap();

    update(tmp.path()).unwrap();

    let manifest =
        fs::read_to_string(tmp.path().join(MIGRATION_MANIFESTS[0].checksums_file)).unwrap();
    assert!(manifest.contains("0001_create_tables.sql"), "{manifest}");
    assert!(manifest.contains("0002_add_status.sql"), "{manifest}");
    assert!(manifest.contains("0003_add_kind.sql"), "{manifest}");
    assert_eq!(check_inner(tmp.path()).unwrap(), 4);
}

#[test]
fn render_manifest_documents_append_only_rule() {
    let manifest = render_manifest(
        &MIGRATION_MANIFESTS[0],
        &[MigrationEntry {
            name: "0001_create_tables.sql".to_owned(),
            checksum: "a".repeat(96),
        }],
    );

    assert!(
        manifest.contains("Migrations are append-only once merged"),
        "{manifest}"
    );
    assert!(manifest.ends_with(&format!("0001_create_tables.sql {}\n", "a".repeat(96))));
}

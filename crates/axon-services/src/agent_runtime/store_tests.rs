use super::*;

#[test]
fn store_path_is_fixed_inside_canonical_config_directory() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let configured = directory.path().join("custom.sqlite3");

    let path = resolve_store_path(&configured).expect("resolve store path");

    assert_eq!(
        path,
        directory
            .path()
            .canonicalize()
            .expect("canonical temporary directory")
            .join("agent-turns.sqlite3")
    );
}

#[test]
fn store_path_rejects_a_missing_config_directory() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let configured = directory.path().join("missing").join("custom.sqlite3");

    assert!(resolve_store_path(&configured).is_err());
}

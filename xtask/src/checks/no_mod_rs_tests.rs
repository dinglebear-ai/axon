use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn find_mod_rs_in_temp_dir() {
    let dir = tempdir().expect("create tempdir");
    let sub = dir.path().join("foo");
    fs::create_dir_all(&sub).expect("mkdir");
    fs::write(sub.join("mod.rs"), "// legacy\n").expect("write mod.rs");

    let result = check(dir.path());
    assert!(result.is_err(), "expected error when mod.rs is present");
}

#[test]
fn passes_when_no_mod_rs() {
    let dir = tempdir().expect("create tempdir");
    let sub = dir.path().join("foo");
    fs::create_dir_all(&sub).expect("mkdir");
    fs::write(sub.join("bar.rs"), "// modern\n").expect("write bar.rs");

    let result = check(dir.path());
    assert!(
        result.is_ok(),
        "expected ok with no mod.rs files: {result:?}"
    );
}

#[test]
fn ignores_target_dir_entries() {
    let dir = tempdir().expect("create tempdir");
    let target = dir.path().join("target").join("foo");
    fs::create_dir_all(&target).expect("mkdir target");
    fs::write(target.join("mod.rs"), "// build artifact\n").expect("write");

    let result = check(dir.path());
    assert!(
        result.is_ok(),
        "expected target/ entries to be skipped: {result:?}"
    );
}

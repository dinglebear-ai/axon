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

#[test]
fn ignores_vendored_and_cargo_registry_entries() {
    let dir = tempdir().expect("create tempdir");
    // Repo-local CARGO_HOME registry cache: upstream crate sources use mod.rs.
    let registry = dir
        .path()
        .join(".cargo")
        .join("registry")
        .join("src")
        .join("some-crate-1.0.0")
        .join("stream");
    fs::create_dir_all(&registry).expect("mkdir .cargo registry");
    fs::write(registry.join("mod.rs"), "// vendored dep\n").expect("write");
    // `cargo vendor` output likewise.
    let vendor = dir.path().join("vendor").join("other-crate").join("io");
    fs::create_dir_all(&vendor).expect("mkdir vendor");
    fs::write(vendor.join("mod.rs"), "// vendored dep\n").expect("write");

    let result = check(dir.path());
    assert!(
        result.is_ok(),
        "expected .cargo/ and vendor/ entries to be skipped: {result:?}"
    );
}

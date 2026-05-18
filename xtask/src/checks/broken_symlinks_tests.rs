use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::tempdir;

#[test]
fn detects_broken_symlink() {
    let dir = tempdir().expect("create tempdir");
    symlink("does-not-exist", dir.path().join("link")).expect("symlink");

    let result = check(dir.path());
    assert!(result.is_err(), "expected error on broken symlink");
}

#[test]
fn passes_when_symlink_target_exists() {
    let dir = tempdir().expect("create tempdir");
    fs::write(dir.path().join("real.txt"), "x").expect("write");
    symlink("real.txt", dir.path().join("link")).expect("symlink");

    let result = check(dir.path());
    assert!(result.is_ok(), "expected ok: {result:?}");
}

#[test]
fn ignores_skipped_dirs() {
    let dir = tempdir().expect("create tempdir");
    let nested = dir.path().join("target");
    fs::create_dir_all(&nested).expect("mkdir");
    symlink("does-not-exist", nested.join("link")).expect("symlink");

    let result = check(dir.path());
    assert!(result.is_ok(), "expected target/ to be skipped: {result:?}");
}

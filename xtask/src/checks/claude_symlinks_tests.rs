#![cfg(unix)]
use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::tempdir;

fn write_claude(dir: &Path) {
    fs::write(dir.join("CLAUDE.md"), "# claude\n").expect("write CLAUDE.md");
}

fn make_valid_symlinks(dir: &Path) {
    symlink("CLAUDE.md", dir.join("AGENTS.md")).expect("symlink agents");
    symlink("CLAUDE.md", dir.join("GEMINI.md")).expect("symlink gemini");
}

#[test]
fn passes_with_proper_symlinks() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    make_valid_symlinks(dir.path());
    let result = check(dir.path());
    assert!(result.is_ok(), "expected ok: {result:?}");
}

#[test]
fn fails_when_agents_missing() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    let result = check(dir.path());
    let err = result.expect_err("expected failure");
    let msg = format!("{err}");
    assert!(msg.contains("claude-symlinks failure"), "msg={msg}");
}

#[test]
fn fails_when_not_symlink() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    // Regular file, not symlink.
    fs::write(dir.path().join("AGENTS.md"), "# regular\n").expect("write file");
    symlink("CLAUDE.md", dir.path().join("GEMINI.md")).expect("symlink gemini");
    let result = check(dir.path());
    assert!(result.is_err(), "expected failure");
}

#[test]
fn fails_when_wrong_target() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    symlink("OTHER.md", dir.path().join("AGENTS.md")).expect("symlink wrong");
    symlink("CLAUDE.md", dir.path().join("GEMINI.md")).expect("symlink ok");
    let result = check(dir.path());
    assert!(result.is_err(), "expected failure");
}

#[test]
fn walks_nested_claude_md() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    make_valid_symlinks(dir.path());
    let sub = dir.path().join("sub");
    fs::create_dir_all(&sub).expect("mkdir sub");
    write_claude(&sub);
    // Nested CLAUDE.md without symlinks should fail.
    let result = check(dir.path());
    assert!(result.is_err(), "expected nested missing to fail");
}

#[test]
fn skips_excluded_dirs() {
    let dir = tempdir().expect("create tempdir");
    write_claude(dir.path());
    make_valid_symlinks(dir.path());
    let target_dir = dir.path().join("target").join("foo");
    fs::create_dir_all(&target_dir).expect("mkdir target");
    write_claude(&target_dir);
    // No symlinks in target/foo, but it must be skipped.
    let result = check(dir.path());
    assert!(result.is_ok(), "expected target/ to be skipped: {result:?}");
}

#![cfg(unix)]
use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::tempdir;

fn write_agents(dir: &Path) {
    fs::write(dir.join("AGENTS.md"), "# agents\n").expect("write AGENTS.md");
}

fn make_valid_symlinks(dir: &Path) {
    symlink("AGENTS.md", dir.join("CLAUDE.md")).expect("symlink agents");
    symlink("AGENTS.md", dir.join("GEMINI.md")).expect("symlink gemini");
}

#[test]
fn passes_with_proper_symlinks() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    let result = check(dir.path());
    assert!(result.is_ok(), "expected ok: {result:?}");
}

#[test]
fn fails_when_aliases_missing() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    let result = check(dir.path());
    let err = result.expect_err("expected failure");
    let msg = format!("{err}");
    assert!(msg.contains("claude-symlinks failure"), "msg={msg}");
}

#[test]
fn fails_when_not_symlink() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    // Regular file, not symlink.
    fs::write(dir.path().join("CLAUDE.md"), "# regular\n").expect("write file");
    symlink("AGENTS.md", dir.path().join("GEMINI.md")).expect("symlink gemini");
    let result = check(dir.path());
    assert!(result.is_err(), "expected failure");
}

#[test]
fn fails_when_wrong_target() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    symlink("OTHER.md", dir.path().join("CLAUDE.md")).expect("symlink wrong");
    symlink("AGENTS.md", dir.path().join("GEMINI.md")).expect("symlink ok");
    let result = check(dir.path());
    assert!(result.is_err(), "expected failure");
}

#[test]
fn walks_nested_agents_md() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    let sub = dir.path().join("sub");
    fs::create_dir_all(&sub).expect("mkdir sub");
    write_agents(&sub);
    // Nested AGENTS.md without symlinks should fail.
    let result = check(dir.path());
    assert!(result.is_err(), "expected nested missing to fail");
}

#[test]
fn skips_excluded_dirs() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    let target_dir = dir.path().join("target").join("foo");
    fs::create_dir_all(&target_dir).expect("mkdir target");
    write_agents(&target_dir);
    // No symlinks in target/foo, but it must be skipped.
    let result = check(dir.path());
    assert!(result.is_ok(), "expected target/ to be skipped: {result:?}");
}

#[test]
fn skips_immutable_full_review_snapshots() {
    let dir = tempdir().expect("create tempdir");
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    for root in [
        ".full-review",
        ".full-review-archive",
        ".full-review-archive-2026-09-08",
    ] {
        let snapshot = dir
            .path()
            .join(root)
            .join("scope-files/crates/axon-services/src");
        fs::create_dir_all(&snapshot).expect("mkdir immutable snapshot");
        write_agents(&snapshot);
    }

    let result = check(dir.path());
    assert!(
        result.is_ok(),
        "immutable .full-review snapshots must be excluded: {result:?}"
    );
}

#[test]
fn rejects_legacy_reversed_layout() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("CLAUDE.md"), "# legacy\n").unwrap();
    symlink("CLAUDE.md", dir.path().join("AGENTS.md")).unwrap();
    symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_missing_root_guidance() {
    let dir = tempdir().unwrap();
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_orphan_aliases() {
    let dir = tempdir().unwrap();
    make_valid_symlinks(dir.path());
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_symlinked_canonical_file() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("other.md"), "# guidance\n").unwrap();
    symlink("other.md", dir.path().join("AGENTS.md")).unwrap();
    make_valid_symlinks(dir.path());
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_absolute_alias_even_when_it_resolves() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    symlink(dir.path().join("AGENTS.md"), dir.path().join("CLAUDE.md")).unwrap();
    symlink("AGENTS.md", dir.path().join("GEMINI.md")).unwrap();
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_chained_aliases() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    symlink("AGENTS.md", dir.path().join("CLAUDE.md")).unwrap();
    symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
    assert!(check(dir.path()).is_err());
}

#[test]
fn rejects_alias_cycles_without_following_them() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    symlink("GEMINI.md", dir.path().join("CLAUDE.md")).unwrap();
    symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
    assert!(check(dir.path()).is_err());
}

#[test]
fn skips_other_worktrees() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    let other = dir.path().join(".worktrees/other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("CLAUDE.md"), "# other branch\n").unwrap();
    assert!(check(dir.path()).is_ok());
}

#[test]
fn skips_downloaded_cargo_dependency_guidance() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    for cache in [
        ".cargo/registry/src/dependency",
        ".cargo/git/checkouts/dependency",
    ] {
        let dependency = dir.path().join(cache);
        fs::create_dir_all(&dependency).unwrap();
        fs::write(dependency.join("AGENTS.md"), "# third-party guidance\n").unwrap();
    }
    assert!(check(dir.path()).is_ok());
}

#[test]
fn still_checks_repository_cargo_guidance() {
    let dir = tempdir().unwrap();
    write_agents(dir.path());
    make_valid_symlinks(dir.path());
    let cargo = dir.path().join(".cargo");
    fs::create_dir_all(&cargo).unwrap();
    write_agents(&cargo);
    assert!(check(dir.path()).is_err());
}

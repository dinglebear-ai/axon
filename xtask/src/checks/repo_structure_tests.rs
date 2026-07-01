use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::repo_structure::{
    EXISTING_STABLE_CRATES, TARGET_NEW_CRATES, TRANSITIONAL_CRATES, check_root,
};

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn symlink(target: &str, link: &Path) {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    unix_fs::symlink(target, link).unwrap();
}

fn complete_fixture() -> PathBuf {
    let dir = tempdir().unwrap().keep();
    let all_crates = TARGET_NEW_CRATES
        .iter()
        .chain(TRANSITIONAL_CRATES.iter())
        .chain(EXISTING_STABLE_CRATES.iter())
        .copied()
        .collect::<Vec<_>>();
    let members = all_crates
        .iter()
        .map(|krate| format!("    \"crates/{krate}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &dir.join("Cargo.toml"),
        &format!("[workspace]\nmembers = [\n{members}\n]\n"),
    );

    for krate in all_crates {
        let root = dir.join("crates").join(krate);
        write(&root.join("Cargo.toml"), "[package]\nname = \"fixture\"\n");
        write(
            &root.join("src/lib.rs"),
            "pub const CRATE_NAME: &str = \"fixture\";\n",
        );
        write(&root.join("src/CLAUDE.md"), "# Fixture\n");
        symlink("CLAUDE.md", &root.join("src/AGENTS.md"));
        symlink("CLAUDE.md", &root.join("src/GEMINI.md"));
    }

    dir
}

#[test]
fn complete_fixture_passes() {
    let root = complete_fixture();
    check_root(&root).unwrap();
}

#[test]
fn missing_target_crate_fails() {
    let root = complete_fixture();
    fs::remove_dir_all(root.join("crates/axon-prune")).unwrap();

    let err = check_root(&root).unwrap_err();
    assert!(
        err.contains("missing target crate directory: crates/axon-prune"),
        "{err}"
    );
}

#[test]
fn broken_agent_memory_symlink_fails() {
    let root = complete_fixture();
    fs::remove_file(root.join("crates/axon-route/src/AGENTS.md")).unwrap();
    symlink(
        "../CLAUDE.md",
        &root.join("crates/axon-route/src/AGENTS.md"),
    );

    let err = check_root(&root).unwrap_err();
    assert!(
        err.contains("crates/axon-route/src/AGENTS.md must symlink to CLAUDE.md"),
        "{err}"
    );
}

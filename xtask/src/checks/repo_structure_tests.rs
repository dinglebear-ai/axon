use std::fs;
use std::path::{Path, PathBuf};

use tempfile::{TempDir, tempdir};

use super::repo_structure::{TARGET_CRATES, check_root};

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
    symlink_file(target, link);
}

#[cfg(unix)]
fn symlink_file(target: &str, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn symlink_file(target: &str, link: &Path) {
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
}

fn complete_fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let all_crates = TARGET_CRATES
        .iter()
        .map(|krate| krate.name)
        .chain(["axon-api", "axon-crawl"])
        .collect::<Vec<_>>();
    let members = all_crates
        .iter()
        .map(|krate| format!("    \"crates/{krate}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &root.join("Cargo.toml"),
        &format!("[workspace]\nmembers = [\n{members}\n]\n"),
    );

    for krate in all_crates {
        let crate_root = root.join("crates").join(krate);
        write(
            &crate_root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n\n[dependencies]\n",
        );
        write(
            &crate_root.join("src/lib.rs"),
            "pub const CRATE_NAME: &str = \"fixture\";\n",
        );
        write(&crate_root.join("src/CLAUDE.md"), "# Fixture\n");
        symlink("CLAUDE.md", &crate_root.join("src/AGENTS.md"));
        symlink("CLAUDE.md", &crate_root.join("src/GEMINI.md"));
    }

    for krate in TARGET_CRATES {
        let crate_root = root.join("crates").join(krate.name);
        let lib_rs = krate
            .modules
            .iter()
            .map(|module| format!("pub mod {module};"))
            .chain([format!("pub const CRATE_NAME: &str = \"{}\";", krate.name)])
            .collect::<Vec<_>>()
            .join("\n");
        write(&crate_root.join("src/lib.rs"), &format!("{lib_rs}\n"));

        for module in krate.modules {
            write(
                &crate_root.join("src").join(format!("{module}.rs")),
                &format!("pub const MODULE_NAME: &str = \"{module}\";\n"),
            );
        }
    }

    Fixture { _dir: dir, root }
}

#[test]
fn complete_fixture_passes() {
    let fixture = complete_fixture();
    check_root(&fixture.root).unwrap();
}

#[test]
fn missing_target_crate_fails() {
    let fixture = complete_fixture();
    fs::remove_dir_all(fixture.root.join("crates/axon-prune")).unwrap();

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("workspace member path does not exist: crates/axon-prune"),
        "{err}"
    );
}

#[test]
fn broken_agent_memory_symlink_fails() {
    let fixture = complete_fixture();
    fs::remove_file(fixture.root.join("crates/axon-route/src/AGENTS.md")).unwrap();
    symlink(
        "../CLAUDE.md",
        &fixture.root.join("crates/axon-route/src/AGENTS.md"),
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("crates/axon-route/src/AGENTS.md must symlink to CLAUDE.md"),
        "{err}"
    );
}

#[test]
fn missing_target_module_fails() {
    let fixture = complete_fixture();
    fs::remove_file(fixture.root.join("crates/axon-route/src/resolver.rs")).unwrap();

    let err = check_root(&fixture.root).unwrap_err();
    assert!(err.contains("crates/axon-route/src/resolver.rs"), "{err}");
}

#[test]
fn missing_target_module_declaration_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-route/src/lib.rs"),
        "pub const CRATE_NAME: &str = \"axon-route\";\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains(
            "crates/axon-route/src/lib.rs is missing module declaration: pub mod resolver;"
        ),
        "{err}"
    );
}

#[test]
fn target_dependency_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-error/Cargo.toml"),
        "[package]\nname = \"fixture\"\n\n[dependencies]\naxon-services = { path = \"../axon-services\" }\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("PR0 target crate axon-error must keep [dependencies] empty"),
        "{err}"
    );
}

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::symlink as make_symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as make_symlink;
use tempfile::{TempDir, tempdir};

use super::repo_structure::{REQUIRED_WORKSPACE_MEMBERS, TARGET_CRATES, check_root};

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn write_symlink(target: &str, link: &Path) {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    make_symlink(target, link).unwrap();
}

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
}

fn complete_fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let members = REQUIRED_WORKSPACE_MEMBERS
        .iter()
        .map(|member| (*member).to_string())
        .chain(
            TARGET_CRATES
                .iter()
                .map(|krate| format!("crates/{}", krate.name)),
        )
        .collect::<Vec<_>>();
    let members_toml = members
        .iter()
        .map(|member| format!("    \"{member}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &root.join("Cargo.toml"),
        &format!(
            "[workspace]\nmembers = [\n{members_toml}\n]\n\n[workspace.package]\nrust-version = \"1.94.0\"\n"
        ),
    );

    for member in members {
        let crate_root = root.join(member);
        let package_name = crate_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap();
        write(
            &crate_root.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{package_name}\"\nrust-version.workspace = true\n\n[dependencies]\n"
            ),
        );
        write(
            &crate_root.join("src/lib.rs"),
            "pub const CRATE_NAME: &str = \"fixture\";\n",
        );
        write(&crate_root.join("src/CLAUDE.md"), "# Fixture\n");
        write_symlink("CLAUDE.md", &crate_root.join("src/AGENTS.md"));
        write_symlink("CLAUDE.md", &crate_root.join("src/GEMINI.md"));
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
    fs::remove_file(fixture.root.join("crates/axon-prune/src/AGENTS.md")).unwrap();
    write_symlink(
        "../CLAUDE.md",
        &fixture.root.join("crates/axon-prune/src/AGENTS.md"),
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(err.contains("AGENTS.md must symlink to CLAUDE.md"), "{err}");
}

#[test]
fn missing_target_module_fails() {
    let fixture = complete_fixture();
    fs::remove_file(fixture.root.join("crates/axon-prune/src/plan.rs")).unwrap();

    let err = check_root(&fixture.root).unwrap_err();
    assert!(err.contains("plan.rs"), "{err}");
}

#[test]
fn missing_target_module_declaration_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/src/lib.rs"),
        "pub const CRATE_NAME: &str = \"axon-prune\";\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("lib.rs is missing module declaration: pub mod plan;"),
        "{err}"
    );
}

#[test]
fn unexpected_target_module_declaration_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/src/lib.rs"),
        "pub mod plan;\npub mod surprise;\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("lib.rs declares unexpected PR0 public module: pub mod surprise;"),
        "{err}"
    );
}

#[test]
fn unexpected_target_module_file_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/src/surprise.rs"),
        "pub const MODULE_NAME: &str = \"surprise\";\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("surprise.rs is an unexpected PR0 module file"),
        "{err}"
    );
}

#[test]
fn target_dependency_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/Cargo.toml"),
        "[package]\nname = \"axon-prune\"\nrust-version.workspace = true\n\n[dependencies]\naxon-services = { path = \"../axon-services\" }\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("PR0 target crate axon-prune must keep [dependencies] empty"),
        "{err}"
    );
}

#[test]
fn target_specific_dependency_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/Cargo.toml"),
        "[package]\nname = \"axon-prune\"\nrust-version.workspace = true\n\n[target.'cfg(unix)'.dependencies]\naxon-services = { path = \"../axon-services\" }\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("PR0 target crate axon-prune must keep [target.cfg(unix).dependencies] empty"),
        "{err}"
    );
}

#[test]
fn package_metadata_dependencies_are_allowed() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/Cargo.toml"),
        "[package]\nname = \"axon-prune\"\nrust-version.workspace = true\n\n[package.metadata.dependencies]\nnotes = \"not a Cargo dependency table\"\n\n[dependencies]\n",
    );

    check_root(&fixture.root).unwrap();
}

#[test]
fn target_package_name_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/Cargo.toml"),
        "[package]\nname = \"fixture\"\nrust-version.workspace = true\n\n[dependencies]\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("PR0 target crate axon-prune must set package.name = \"axon-prune\""),
        "{err}"
    );
}

#[test]
fn missing_target_rust_version_fails() {
    let fixture = complete_fixture();
    write(
        &fixture.root.join("crates/axon-prune/Cargo.toml"),
        "[package]\nname = \"axon-prune\"\n\n[dependencies]\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("PR0 target crate axon-prune must set rust-version.workspace = true"),
        "{err}"
    );
}

#[test]
fn missing_required_workspace_member_fails() {
    let fixture = complete_fixture();
    let members = REQUIRED_WORKSPACE_MEMBERS
        .iter()
        .map(|member| (*member).to_string())
        .filter(|member| *member != "crates/axon-cli")
        .chain(
            TARGET_CRATES
                .iter()
                .map(|krate| format!("crates/{}", krate.name)),
        )
        .map(|member| format!("    \"{member}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &fixture.root.join("Cargo.toml"),
        &format!(
            "[workspace]\nmembers = [\n{members}\n]\n\n[workspace.package]\nrust-version = \"1.94.0\"\n"
        ),
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("root Cargo.toml is missing workspace member: crates/axon-cli"),
        "{err}"
    );
}

#[test]
fn unexpected_workspace_member_fails() {
    let fixture = complete_fixture();
    fs::create_dir_all(fixture.root.join("crates/axon-surprise")).unwrap();
    write(
        &fixture.root.join("Cargo.toml"),
        "[workspace]\nmembers = [\n    \"xtask\",\n    \"crates/axon-api\",\n    \"crates/axon-authz\",\n    \"crates/axon-core\",\n    \"crates/axon-crawl\",\n    \"crates/axon-vector\",\n    \"crates/axon-ingest\",\n    \"crates/axon-extract\",\n    \"crates/axon-jobs\",\n    \"crates/axon-code-index\",\n    \"crates/axon-services\",\n    \"crates/axon-mcp\",\n    \"crates/axon-web\",\n    \"crates/axon-cli\",\n    \"crates/axon-error\",\n    \"crates/axon-observe\",\n    \"crates/axon-route\",\n    \"crates/axon-adapters\",\n    \"crates/axon-ledger\",\n    \"crates/axon-parse\",\n    \"crates/axon-graph\",\n    \"crates/axon-memory\",\n    \"crates/axon-document\",\n    \"crates/axon-embedding\",\n    \"crates/axon-vectors\",\n    \"crates/axon-retrieval\",\n    \"crates/axon-llm\",\n    \"crates/axon-prune\",\n    \"crates/axon-surprise\",\n]\n\n[workspace.package]\nrust-version = \"1.94.0\"\n",
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("unexpected PR0 workspace member: crates/axon-surprise"),
        "{err}"
    );
}

#[test]
fn missing_workspace_rust_version_fails() {
    let fixture = complete_fixture();
    let members = TARGET_CRATES
        .iter()
        .map(|krate| format!("    \"crates/{}\",", krate.name))
        .chain(["    \"crates/axon-api\",".to_string()])
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &fixture.root.join("Cargo.toml"),
        &format!("[workspace]\nmembers = [\n{members}\n]\n"),
    );

    let err = check_root(&fixture.root).unwrap_err();
    assert!(
        err.contains("root Cargo.toml must set workspace.package.rust-version = \"1.94.0\""),
        "{err}"
    );
}

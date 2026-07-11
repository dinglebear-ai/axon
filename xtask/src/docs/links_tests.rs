use super::*;
use std::fs;

#[test]
fn finds_broken_links_outside_docs_reference() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("README.md"),
        "[missing](./nope.md) [ok](#anchor)",
    )
    .unwrap();
    let err = check_repo_wide(dir.path()).unwrap_err();
    assert!(err.to_string().contains("README.md"));
    assert!(err.to_string().contains("nope.md"));
}

#[test]
fn passes_when_links_resolve_across_the_repo() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/guides")).unwrap();
    fs::write(dir.path().join("docs/guides/quickstart.md"), "# hi").unwrap();
    fs::write(
        dir.path().join("README.md"),
        "[quickstart](docs/guides/quickstart.md)",
    )
    .unwrap();
    check_repo_wide(dir.path()).unwrap();
}

#[test]
fn skips_noise_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("target/doc")).unwrap();
    fs::write(
        dir.path().join("target/doc/broken.md"),
        "[missing](./nope.md)",
    )
    .unwrap();
    check_repo_wide(dir.path()).unwrap();
}

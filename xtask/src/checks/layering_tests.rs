use std::fs;
use std::path::Path;

use tempfile::tempdir;

use super::layering::check;

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn write_surface_fixture(root: &Path) {
    for surface in ["axon-cli", "axon-web", "axon-mcp"] {
        write(
            &root.join("crates").join(surface).join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{surface}\"\nversion = \"0.0.0\"\n\n[dependencies]\naxon-services = {{ path = \"../axon-services\" }}\n"
            ),
        );
        write(
            &root.join("crates").join(surface).join("src/lib.rs"),
            "pub const OK: bool = true;\n",
        );
    }
}

#[test]
fn surface_crates_cannot_depend_on_pr9_provider_crates_before_cutover() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/Cargo.toml"),
        "[package]\nname = \"axon-cli\"\nversion = \"0.0.0\"\n\n[dependencies]\naxon-services = { path = \"../axon-services\" }\naxon-retrieval = { path = \"../axon-retrieval\" }\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-cli/Cargo.toml declares [dependencies] dependency on `axon-retrieval` before the public cutover"
        ),
        "{err}"
    );
}

#[test]
fn surface_crates_without_pr9_provider_dependencies_pass_cutover_guard() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());

    check(temp.path()).unwrap();
}

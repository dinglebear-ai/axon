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
fn surface_crates_cannot_depend_on_forbidden_domain_crates() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/Cargo.toml"),
        "[package]\nname = \"axon-cli\"\nversion = \"0.0.0\"\n\n[dependencies]\naxon-services = { path = \"../axon-services\" }\naxon-retrieval = { path = \"../axon-retrieval\" }\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-cli/Cargo.toml declares [dependencies] dependency on `axon-retrieval` — transports must go through the axon-services facade"
        ),
        "{err}"
    );
}

#[test]
fn surface_crates_without_forbidden_domain_dependencies_pass() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());

    check(temp.path()).unwrap();
}

#[test]
fn transport_crate_reaching_into_domain_internal_module_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/src/lib.rs"),
        "pub use axon_vectors::qdrant::QdrantVectorStore;\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-cli/src/lib.rs:1  reaches domain internal `axon_vectors::qdrant::`"
        ),
        "{err}"
    );
}

#[test]
fn services_crate_reaching_into_web_engine_internals_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub use axon_adapters::web_engine::scrape::scrape_to_result;\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-services/src/lib.rs:1  reaches domain internal `axon_adapters::web_engine::`"
        ),
        "{err}"
    );
}

#[test]
fn transport_crate_importing_llm_provider_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-web/src/lib.rs"),
        "use axon_llm::{self as llm, CompletionRequest};\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains("crates/axon-web/src/lib.rs:1  reaches provider crate `axon_llm::`"),
        "{err}"
    );
}

#[test]
fn transport_crate_reaching_into_source_internals_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-mcp/src/lib.rs"),
        "use axon_services::source::execution::execute_source;\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-mcp/src/lib.rs:1  reaches service source internal `axon_services::source::execution::`"
        ),
        "{err}"
    );
}

#[test]
fn services_raw_provider_call_outside_reserved_facade_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp
            .path()
            .join("crates/axon-services/src/source/new_pipeline.rs"),
        "let embeddings = embedding_provider.embed(batch).await?;\n",
    );

    let err = check(temp.path()).unwrap_err().to_string();

    assert!(
        err.contains(
            "crates/axon-services/src/source/new_pipeline.rs:1  calls raw provider via `embedding_provider.embed(`"
        ),
        "{err}"
    );
}

#[test]
fn temporary_path_scoped_exception_does_not_hide_other_reaches() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp
            .path()
            .join("crates/axon-cli/src/commands/screenshot/util.rs"),
        "pub(crate) use axon_adapters::web_engine::screenshot::url_to_screenshot_filename;\n",
    );

    check(temp.path()).unwrap();

    write(
        &temp.path().join("crates/axon-cli/src/commands/other.rs"),
        "pub(crate) use axon_adapters::web_engine::screenshot::url_to_screenshot_filename;\n",
    );
    let err = check(temp.path()).unwrap_err().to_string();
    assert!(
        err.contains("crates/axon-cli/src/commands/other.rs:1"),
        "{err}"
    );
}

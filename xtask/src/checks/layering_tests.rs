use std::fs;
use std::path::Path;

use tempfile::tempdir;

use super::layering::{
    ManifestException, ReachException, check_fixture as check, check_fixture_with_exceptions,
};

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
    write(
        &root.join("crates/axon-services/src/lib.rs"),
        "pub const OK: bool = true;\n",
    );
}

#[test]
fn clean_surfaces_pass() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    check(temp.path()).unwrap();
}

#[test]
fn forbidden_dependency_fails_and_names_its_table() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/Cargo.toml"),
        "[package]\nname='axon-cli'\nversion='0.0.0'\n[dev-dependencies]\naxon-retrieval={path='../axon-retrieval'}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains(
            "crates/axon-cli/Cargo.toml declares [dev-dependencies] dependency on `axon-retrieval`"
        ),
        "{error}"
    );
}

#[test]
fn grouped_multiline_and_renamed_imports_fail() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/src/lib.rs"),
        "use axon_vectors::{\n    qdrant::{QdrantVectorStore as Store},\n};\npub fn use_it(_: Store) {}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("[reach:axon_vectors::qdrant]"), "{error}");
}

#[test]
fn imported_provider_type_alias_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_embedding::EmbeddingProvider as EP;\npub fn wire(_: &dyn EP) {}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-type:EmbeddingProvider]"),
        "{error}"
    );
}

#[test]
fn provider_handle_alias_fails_without_receiver_name_dependency() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub async fn run(runtime: Runtime) {\n    let arbitrary = runtime.embedding_provider;\n    arbitrary.embed(vec![]).await;\n}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-handle:embedding_provider]"),
        "{error}"
    );
}

#[test]
fn provider_ufcs_through_renamed_trait_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors::VectorStore as VS;\npub async fn run(store: &dyn VS) {\n    VS::upsert(store, vec![]).await;\n}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("[provider-type:VectorStore]"), "{error}");
    assert!(
        error.contains("[provider-op:VectorStore::upsert]"),
        "{error}"
    );
}

#[test]
fn renamed_llm_module_call_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_llm as engine;\npub async fn run(request: Request) { engine::complete_text(request).await; }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-op:axon_llm::complete_text]"),
        "{error}"
    );
}

#[test]
fn comments_block_comments_and_strings_do_not_trigger() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "/* use axon_embedding::EmbeddingProvider; */\npub const TEXT: &str = \"runtime.embedding_provider.embed()\";\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn malformed_rust_fails_closed() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-web/src/lib.rs"),
        "pub fn broken( {\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("failed to parse Rust source"), "{error}");
}

#[test]
fn malformed_or_missing_manifest_fails_closed() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-mcp/Cargo.toml"),
        "[dependencies\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("failed to parse manifest"), "{error}");
}

#[test]
fn missing_manifest_and_source_tree_fail_closed() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    fs::remove_file(temp.path().join("crates/axon-mcp/Cargo.toml")).unwrap();
    fs::remove_dir_all(temp.path().join("crates/axon-services/src")).unwrap();
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("failed to read manifest"), "{error}");
    assert!(error.contains("failed to walk source tree"), "{error}");
}

#[test]
fn exact_reach_exception_rejects_excess_count() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    let path = "crates/axon-services/src/lib.rs";
    write(
        &temp.path().join(path),
        "use axon_embedding::EmbeddingProvider;\npub fn two(_: &dyn EmbeddingProvider, _: &dyn EmbeddingProvider) {}\n",
    );
    let exceptions = [ReachException {
        path,
        rule: "provider-type:EmbeddingProvider",
        owner: "axon_rust-test",
        expected_count: 1,
    }];
    let error = check_fixture_with_exceptions(temp.path(), &exceptions, &[])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("expected 1 occurrence(s), found 3"),
        "{error}"
    );
}

#[test]
fn stale_exception_and_missing_owner_fail() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    let exceptions = [ReachException {
        path: "crates/axon-services/src/lib.rs",
        rule: "provider-type:EmbeddingProvider",
        owner: "",
        expected_count: 1,
    }];
    let error = check_fixture_with_exceptions(temp.path(), &exceptions, &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing or invalid owner"), "{error}");
    assert!(error.contains("found 0"), "{error}");
}

#[test]
fn duplicate_exception_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    let exception = ReachException {
        path: "crates/axon-services/src/lib.rs",
        rule: "provider-handle:embedding_provider",
        owner: "axon_rust-test",
        expected_count: 1,
    };
    let error = check_fixture_with_exceptions(temp.path(), &[exception, exception], &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate exception"), "{error}");
}

#[test]
fn manifest_exception_is_bound_to_exact_table() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/Cargo.toml"),
        "[package]\nname='axon-cli'\nversion='0.0.0'\n[dev-dependencies]\naxon-adapters={path='../axon-adapters'}\n",
    );
    let exceptions = [ManifestException {
        path: "crates/axon-cli/Cargo.toml",
        dependency: "axon-adapters",
        table: "dependencies",
        owner: "axon_rust-test",
        expected_count: 1,
    }];
    let error = check_fixture_with_exceptions(temp.path(), &[], &exceptions)
        .unwrap_err()
        .to_string();
    assert!(error.contains("[dependencies] exception"), "{error}");
    assert!(error.contains("declares [dev-dependencies]"), "{error}");
}

#[test]
fn fixed_reserved_facade_allows_provider_access() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp
            .path()
            .join("crates/axon-services/src/reserved_call.rs"),
        "use axon_embedding::EmbeddingProvider;\npub async fn run(runtime: Runtime) { runtime.embedding_provider.embed(vec![]).await; }\n",
    );
    check(temp.path()).unwrap();
}

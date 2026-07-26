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
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = []\n[workspace.dependencies]\n",
    );
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
fn low_collision_provider_method_fails_without_receiver_type() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub async fn run(arbitrary: Unknown) { arbitrary.embed(vec![]).await; }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("[provider-method:embed]"), "{error}");
}

#[test]
fn provider_qualified_collision_prone_ufcs_operation_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors::VectorStore;\npub async fn run(store: &dyn VectorStore) { VectorStore::delete(store, selector()).await; }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-op:VectorStore::delete]"),
        "{error}"
    );
}

#[test]
fn provider_glob_import_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_embedding::*;\npub fn run() {}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("[provider-glob:axon_embedding]"), "{error}");
}

#[test]
fn provider_named_destructuring_fails() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub fn run(runtime: Runtime) { let Runtime { embedding_provider: arbitrary, .. } = runtime; consume(arbitrary); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-handle:embedding_provider]"),
        "{error}"
    );
}

#[test]
fn search_and_network_capture_boundaries_fail() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_adapters::{SearchProvider, NetworkCaptureProvider};\npub fn run(runtime: Runtime, _: &dyn SearchProvider, _: &dyn NetworkCaptureProvider) { consume(runtime.search_provider); consume(runtime.network_capture_provider); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-type:SearchProvider",
        "provider-type:NetworkCaptureProvider",
        "provider-handle:search_provider",
        "provider-handle:network_capture_provider",
    ] {
        assert!(error.contains(rule), "missing {rule}: {error}");
    }
}

#[test]
fn macro_tokens_enforce_alias_ufcs_and_method_calls_once() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors::VectorStore as VS;\npub async fn run(store: Store, arbitrary: Unknown) { tokio::select! { _ = VS::delete(&store, selector()) => {}, _ = arbitrary.embed(vec![]) => {} } }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-op:VectorStore::delete]"),
        "{error}"
    );
    assert!(error.contains("[provider-method:embed]"), "{error}");
    assert_eq!(
        error.matches("[provider-op:VectorStore::delete]").count(),
        1,
        "{error}"
    );
    assert_eq!(
        error.matches("[provider-method:embed]").count(),
        1,
        "{error}"
    );
}

#[test]
fn macro_string_and_comment_literals_do_not_trigger() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub fn run(cfg: Config) { trace!(\"embedding_provider.embed() VectorStore::delete\", embed = cfg.embed); /* artifact_store.put_bytes() */ }\n",
    );
    check(temp.path()).unwrap();
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
fn aliases_resolve_before_source_order_and_to_fixed_point() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub async fn run(store: Store) { Alias::delete(&store, selector()).await; }\nuse Base as Alias;\nuse axon_vectors::VectorStore as Base;\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-op:VectorStore::delete]"),
        "{error}"
    );
}

#[test]
fn nested_alias_shadowing_uses_innermost_scope() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors as v;\npub fn run() { use harmless as v; consume(v::qdrant::Value); }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn alias_cycles_terminate_without_false_positive() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use b as a;\nuse a as b;\npub fn run() { consume(a::qdrant::Value); }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn extern_crate_alias_resolves_provider_paths() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "extern crate axon_vectors as v;\npub async fn run(store: Store) { v::VectorStore::delete(&store, selector()).await; }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(
        error.contains("[provider-op:VectorStore::delete]"),
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
fn target_specific_dependency_tables_are_scanned_with_exact_paths() {
    for dependency_kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let temp = tempdir().unwrap();
        write_surface_fixture(temp.path());
        write(
            &temp.path().join("crates/axon-cli/Cargo.toml"),
            &format!(
                "[package]\nname='axon-cli'\nversion='0.0.0'\n[target.'cfg(unix)'.{dependency_kind}]\naxon-vectors={{path='../axon-vectors'}}\n"
            ),
        );
        let error = check(temp.path()).unwrap_err().to_string();
        let table = format!("target.'cfg(unix)'.{dependency_kind}");
        assert!(error.contains(&format!("[{table}]")), "{error}");
    }
}

#[test]
fn target_specific_exception_is_bound_to_full_table_path() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    let manifest = "crates/axon-cli/Cargo.toml";
    write(
        &temp.path().join(manifest),
        "[package]\nname='axon-cli'\nversion='0.0.0'\n[target.'cfg(unix)'.dependencies]\naxon-adapters={path='../axon-adapters'}\n",
    );
    let exact = [ManifestException {
        path: manifest,
        dependency: "axon-adapters",
        table: "target.'cfg(unix)'.dependencies",
        owner: "axon_rust-test",
        expected_count: 1,
    }];
    check_fixture_with_exceptions(temp.path(), &[], &exact).unwrap();

    let wrong_table = [ManifestException {
        table: "dependencies",
        ..exact[0]
    }];
    let error = check_fixture_with_exceptions(temp.path(), &[], &wrong_table)
        .unwrap_err()
        .to_string();
    assert!(error.contains("[dependencies] exception"), "{error}");
    assert!(
        error.contains("declares [target.'cfg(unix)'.dependencies]"),
        "{error}"
    );
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
fn missing_crate_root_fails_closed_even_when_unreachable_rust_files_exist() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    fs::remove_file(temp.path().join("crates/axon-services/src/lib.rs")).unwrap();
    write(
        &temp.path().join("crates/axon-services/src/unreachable.rs"),
        "pub const UNREACHABLE: bool = true;\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("no production crate root found"), "{error}");
}

#[test]
fn cfg_test_items_and_test_only_external_modules_are_excluded() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "#[cfg(test)]\nmod test_support;\n#[cfg(all(test, unix))]\npub fn test_only(runtime: Runtime) { consume(runtime.embedding_provider); }\npub const OK: bool = true;\n",
    );
    write(
        &temp.path().join("crates/axon-services/src/test_support.rs"),
        "pub fn helper(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn mixed_file_still_scans_production_items() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "#[cfg(test)]\npub fn test_only(runtime: Runtime) { consume(runtime.embedding_provider); }\npub fn production(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(!error.contains("embedding_provider"), "{error}");
    assert!(error.contains("provider-handle:vector_store"), "{error}");
}

#[test]
fn production_test_support_module_is_not_hidden_by_filename() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "mod test_support;\npub const OK: bool = true;\n",
    );
    write(
        &temp.path().join("crates/axon-services/src/test_support.rs"),
        "pub fn production(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-handle:vector_store"), "{error}");
}

#[test]
fn cfg_any_test_or_production_is_still_scanned() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "#[cfg(any(test, unix))]\npub fn production_on_unix(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-handle:vector_store"), "{error}");
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

#[test]
fn concrete_provider_bindings_track_collision_prone_calls_and_propagation() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors::QdrantVectorStore;\npub async fn run() {\n    let store = QdrantVectorStore::new(url(), id());\n    store.upsert(batch()).await;\n    store.upsert(batch()).await;\n    let borrowed = &store;\n    borrowed.delete(selector()).await;\n    let cloned = borrowed.clone();\n    cloned.query(request()).await;\n    { let borrowed = Unrelated::new(); borrowed.delete(selector()).await; }\n}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-type:QdrantVectorStore",
        "provider-op:QdrantVectorStore::new",
        "provider-method:delete",
        "provider-method:query",
    ] {
        assert!(error.contains(rule), "missing {rule}: {error}");
    }
    assert_eq!(
        error.matches("[provider-method:upsert]").count(),
        2,
        "{error}"
    );
}

#[test]
fn provider_typed_parameters_and_known_fetch_render_artifact_implementations_are_tracked() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "use axon_vectors::VectorStore;\nuse axon_adapters::{HttpFetchProvider, ChromeRenderProvider};\nuse axon_core::FileArtifactStore;\npub async fn typed(store: &dyn VectorStore) { store.search(request()).await; }\npub async fn concrete() {\n let fetcher = HttpFetchProvider::new(config()); fetcher.fetch(request()).await;\n let renderer = ChromeRenderProvider::new(config()); renderer.render(request()).await;\n let artifacts = FileArtifactStore::new(root()); artifacts.get(handle()).await;\n}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:render",
        "provider-method:get",
        "provider-type:HttpFetchProvider",
        "provider-type:ChromeRenderProvider",
        "provider-type:FileArtifactStore",
    ] {
        assert!(error.contains(rule), "missing {rule}: {error}");
    }
}

#[test]
fn provider_binding_second_call_drifts_exact_exception_count() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    let path = "crates/axon-services/src/lib.rs";
    write(
        &temp.path().join(path),
        "pub async fn run(store: &dyn VectorStore) { store.upsert(batch()).await; store.upsert(batch()).await; }\n",
    );
    let exceptions = [
        ReachException {
            path,
            rule: "provider-type:VectorStore",
            owner: "axon_rust-test",
            expected_count: 1,
        },
        ReachException {
            path,
            rule: "provider-method:upsert",
            owner: "axon_rust-test",
            expected_count: 1,
        },
    ];
    let error = check_fixture_with_exceptions(temp.path(), &exceptions, &[])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("provider-method:upsert")
            && error.contains("expected 1")
            && error.contains("found 2"),
        "{error}"
    );
}

#[test]
fn macro_metadata_keys_are_benign_but_real_bound_provider_calls_are_tracked() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub async fn run(store: &dyn VectorStore, runtime: Runtime) {\n tracing::info!(embedding_provider = \"tei\", artifact_store = ?id);\n tokio::select! { _ = store.upsert(batch()) => {}, _ = consume(runtime.vector_store) => {} }\n}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert_eq!(
        error.matches("[provider-method:upsert]").count(),
        1,
        "{error}"
    );
    assert_eq!(
        error.matches("[provider-handle:vector_store]").count(),
        1,
        "{error}"
    );
    assert!(
        !error.contains("provider-handle:embedding_provider"),
        "{error}"
    );
    assert!(!error.contains("provider-handle:artifact_store"), "{error}");
}

#[test]
fn production_module_reachability_wins_over_cfg_test_alias_to_same_path() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "#[path = \"shared.rs\"]\nmod production;\n#[cfg(test)]\n#[path = \"shared.rs\"]\nmod test_alias;\n",
    );
    write(
        &temp.path().join("crates/axon-services/src/shared.rs"),
        "pub fn production(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-handle:vector_store"), "{error}");
}

#[test]
fn renamed_forbidden_cargo_packages_are_detected_in_normal_and_target_tables() {
    for table in ["dependencies", "target.'cfg(unix)'.build-dependencies"] {
        let temp = tempdir().unwrap();
        write_surface_fixture(temp.path());
        write(
            &temp.path().join("crates/axon-cli/Cargo.toml"),
            &format!(
                "[package]\nname='axon-cli'\nversion='0.0.0'\n[{table}]\nvector-alias={{package='axon-vectors', path='../axon-vectors'}}\n"
            ),
        );
        let error = check(temp.path()).unwrap_err().to_string();
        assert!(error.contains(&format!("[{table}]")), "{error}");
        assert!(error.contains("`axon-vectors`"), "{error}");
        assert!(!error.contains("`vector-alias`"), "{error}");
    }
}

#[test]
fn axon_services_root_glob_is_allowed_while_provider_crate_glob_is_rejected() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-cli/src/lib.rs"),
        "use axon_services::*;\npub fn run() {}\n",
    );
    check(temp.path()).unwrap();

    write(
        &temp.path().join("crates/axon-cli/src/lib.rs"),
        "use axon_vectors::*;\npub fn run() {}\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-glob:axon_vectors"), "{error}");
}

#[test]
fn syntax_visible_provider_laundering_and_pattern_scopes_are_tracked_positionally() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(
    seed: Store,
    pair: (Store, Plain),
    providers: Vec<Store>,
    maybe: Option<Store>,
) {
    let cloned = std::sync::Arc::clone(&seed);
    cloned.upsert(batch()).await;
    let boxed = Box::from(cloned);
    let referenced = Box::as_ref(&boxed);
    referenced.delete(selector()).await;

    let (store, plain) = pair;
    store.search(request()).await;
    plain.search(request()).await;

    let mut slot = plain;
    slot = std::sync::Arc::clone(&seed);
    slot.fetch(request()).await;
    slot = Plain::new();
    slot.render(request()).await;

    if let Some(inner) = maybe {
        inner.get(handle()).await;
    }
    while let Some(inner) = maybe {
        inner.render(request()).await;
    }
    for inner in providers {
        inner.query(request()).await;
    }
    match maybe {
        Some(inner) => inner.delete(selector()).await,
        None => {}
    }
    {
        let cloned = Plain::new();
        cloned.search(request()).await;
    }
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for (rule, count) in [
        ("provider-method:upsert", 1),
        ("provider-method:delete", 2),
        ("provider-method:search", 1),
        ("provider-method:fetch", 1),
        ("provider-method:get", 1),
        ("provider-method:render", 1),
        ("provider-method:query", 1),
    ] {
        assert_eq!(
            error.matches(&format!("[{rule}]")).count(),
            count,
            "{rule}: {error}"
        );
    }
}

#[test]
fn provider_owned_module_prefixes_track_type_shaped_implementations_only() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
pub async fn run() {
    let store = axon_vectors::qdrant::RenamedVectorStore::new();
    store.upsert(batch()).await;
    let unrelated = axon_vectors::qdrant::helper();
    unrelated.search(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert_eq!(
        error.matches("[provider-method:upsert]").count(),
        1,
        "{error}"
    );
    assert!(!error.contains("[provider-method:search]"), "{error}");
}

#[test]
fn custom_helper_return_inference_remains_out_of_scope() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "pub async fn run() { let opaque = custom_provider_helper(); opaque.search(request()).await; }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn workspace_inherited_dependency_aliases_are_canonicalized_in_all_table_shapes() {
    for table in [
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "target.'cfg(unix)'.dependencies",
        "target.'cfg(unix)'.dev-dependencies",
        "target.'cfg(unix)'.build-dependencies",
    ] {
        let temp = tempdir().unwrap();
        write_surface_fixture(temp.path());
        write(
            &temp.path().join("Cargo.toml"),
            "[workspace]\nmembers=[]\n[workspace.dependencies]\nvector-alias={package='axon-vectors', path='crates/axon-vectors'}\n",
        );
        write(
            &temp.path().join("crates/axon-cli/Cargo.toml"),
            &format!(
                "[package]\nname='axon-cli'\nversion='0.0.0'\n[{table}]\nvector-alias={{workspace=true}}\n"
            ),
        );
        let error = check(temp.path()).unwrap_err().to_string();
        assert!(error.contains(&format!("[{table}]")), "{error}");
        assert!(error.contains("`axon-vectors`"), "{error}");
        assert!(!error.contains("dependency on `vector-alias`"), "{error}");
    }
}

#[test]
fn workspace_inheritance_fails_closed_for_missing_or_malformed_definitions() {
    for root_manifest in [
        "[workspace]\nmembers=[]\n[workspace.dependencies]\n",
        "[workspace\nthis is malformed",
        "[workspace]\nmembers=[]\n[workspace.dependencies]\nvector-alias={workspace=true}\n",
        "[workspace]\nmembers=[]\n[workspace.dependencies]\nvector-alias={package=42}\n",
        "[workspace]\nmembers=[]\n[workspace.dependencies]\nvector-alias=[]\n",
    ] {
        let temp = tempdir().unwrap();
        write_surface_fixture(temp.path());
        write(&temp.path().join("Cargo.toml"), root_manifest);
        write(
            &temp.path().join("crates/axon-cli/Cargo.toml"),
            "[package]\nname='axon-cli'\nversion='0.0.0'\n[dependencies]\nvector-alias={workspace=true}\n",
        );
        let error = check(temp.path()).unwrap_err().to_string();
        assert!(
            error.contains("failed to resolve [dependencies] dependency `vector-alias`"),
            "{error}"
        );
    }
}

#[test]
fn nested_inline_module_reachability_preserves_production_and_excludes_test_only_paths() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
mod outer {
    #[path = "shared.rs"]
    mod production;
    #[cfg(test)]
    #[path = "shared.rs"]
    mod test_alias;
    #[cfg(test)]
    mod only_test;
}
"#,
    );
    write(
        &temp.path().join("crates/axon-services/src/outer/shared.rs"),
        "pub fn production(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    write(
        &temp
            .path()
            .join("crates/axon-services/src/outer/only_test.rs"),
        "pub fn test_only(runtime: Runtime) { consume(runtime.embedding_provider); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-handle:vector_store"), "{error}");
    assert!(
        !error.contains("provider-handle:embedding_provider"),
        "{error}"
    );
}

#[test]
fn nested_inline_module_path_overrides_use_the_containing_module_base() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
mod outer {
    #[cfg(test)]
    #[path = "../shared_test.rs"]
    mod test_alias;
}
"#,
    );
    write(
        &temp.path().join("crates/axon-services/src/shared_test.rs"),
        "pub fn test_only(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn provider_assignment_taint_merges_at_branches_matches_and_loop_exits() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, condition: bool, key: u8) {
    let mut then_provider = plain;
    if condition {
        then_provider = std::sync::Arc::clone(&provider);
    } else {
        then_provider = Plain::new();
    }
    then_provider.search(request()).await;

    let mut else_provider = plain;
    if condition {
        else_provider = Plain::new();
    } else {
        else_provider = std::sync::Arc::clone(&provider);
    }
    else_provider.delete(selector()).await;

    let mut matched = plain;
    match key {
        0 => matched = std::sync::Arc::clone(&provider),
        _ => matched = Plain::new(),
    }
    matched.query(request()).await;

    let mut looped = std::sync::Arc::clone(&provider);
    while condition {
        looped = Plain::new();
    }
    looped.fetch(request()).await;

    let mut overwritten_each_iteration = plain;
    while condition {
        overwritten_each_iteration = std::sync::Arc::clone(&provider);
        overwritten_each_iteration = Plain::new();
    }
    overwritten_each_iteration.get(handle()).await;

    let shadowed = plain;
    {
        let mut shadowed = Plain::new();
        shadowed = std::sync::Arc::clone(&provider);
    }
    shadowed.render(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:delete",
        "provider-method:query",
        "provider-method:fetch",
        "provider-method:get",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
    assert!(!error.contains("[provider-method:render]"), "{error}");
}

#[test]
fn straight_line_provider_to_plain_overwrite_clears_collision_taint() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store) {
    let mut sequential = std::sync::Arc::clone(&provider);
    sequential = Plain::new();
    sequential.search(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-type:VectorStore"), "{error}");
    assert!(!error.contains("[provider-method:search]"), "{error}");
}

#[test]
fn guarded_match_false_exit_flows_to_later_arms_without_laundering_body_overwrites() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, key: u8) {
    let mut guarded = plain;
    match key {
        0 if {
            guarded = std::sync::Arc::clone(&provider);
            false
        } => {
            guarded = Plain::new();
        }
        _ => guarded.fetch(request()).await,
    }
    guarded.search(request()).await;

    let mut cleared_by_every_arm = std::sync::Arc::clone(&provider);
    match key {
        0 => cleared_by_every_arm = Plain::new(),
        _ => cleared_by_every_arm = Plain::new(),
    }
    cleared_by_every_arm.get(handle()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in ["provider-method:fetch", "provider-method:search"] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
    assert!(!error.contains("[provider-method:get]"), "{error}");
}

#[test]
fn loop_heads_stabilize_without_duplicating_findings_and_abrupt_exits_keep_taint() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(
    provider: Store,
    plain: Plain,
    condition: bool,
    maybe: Option<u8>,
    items: Vec<u8>,
) {
    let mut while_carried = plain;
    while condition {
        while_carried.search(request()).await;
        while_carried = std::sync::Arc::clone(&provider);
    }

    let mut while_let_carried = plain;
    while let Some(_) = maybe {
        while_let_carried.fetch(request()).await;
        while_let_carried = std::sync::Arc::clone(&provider);
    }

    let mut for_carried = plain;
    for _ in items {
        for_carried.query(request()).await;
        for_carried = std::sync::Arc::clone(&provider);
        continue;
    }

    let mut bare_carried = plain;
    loop {
        bare_carried.get(handle()).await;
        bare_carried = std::sync::Arc::clone(&provider);
        if condition {
            continue;
        }
        break;
    }

    let mut breaking = plain;
    while condition {
        breaking = std::sync::Arc::clone(&provider);
        if condition {
            break;
        }
        breaking = Plain::new();
    }
    breaking.render(request()).await;

    let mut continuing = plain;
    while condition {
        continuing = std::sync::Arc::clone(&provider);
        if condition {
            continue;
        }
        continuing = Plain::new();
    }
    continuing.delete(selector()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
        "provider-method:get",
        "provider-method:render",
        "provider-method:delete",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn closure_and_async_outer_assignment_effects_are_optional_but_bodies_are_scanned() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain) {
    let mut closure_cleared = std::sync::Arc::clone(&provider);
    let _clear = || {
        closure_cleared = Plain::new();
    };
    closure_cleared.search(request()).await;

    let mut closure_injected = plain;
    let _inject = || {
        closure_injected = std::sync::Arc::clone(&provider);
        closure_injected.query(request());
    };
    closure_injected.fetch(request()).await;

    let mut async_cleared = std::sync::Arc::clone(&provider);
    let _clear = async {
        async_cleared = Plain::new();
    };
    async_cleared.render(request()).await;

    let mut async_injected = plain;
    let _inject = async {
        async_injected = std::sync::Arc::clone(&provider);
        async_injected.delete(selector()).await;
    };
    async_injected.get(handle()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:query",
        "provider-method:fetch",
        "provider-method:render",
        "provider-method:delete",
        "provider-method:get",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn short_circuit_rhs_assignment_effects_merge_skipped_and_executed_paths() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, condition: bool) {
    let mut and_cleared = std::sync::Arc::clone(&provider);
    condition && { and_cleared = Plain::new(); true };
    and_cleared.search(request()).await;

    let mut or_cleared = std::sync::Arc::clone(&provider);
    condition || { or_cleared = Plain::new(); false };
    or_cleared.fetch(request()).await;

    let mut and_injected = plain;
    condition && {
        and_injected = std::sync::Arc::clone(&provider);
        true
    };
    and_injected.query(request()).await;

    let mut or_injected = plain;
    condition || {
        or_injected = std::sync::Arc::clone(&provider);
        false
    };
    or_injected.render(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
        "provider-method:render",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn local_block_if_and_match_results_propagate_provider_shape() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, condition: bool, key: u8) {
    let from_block = { std::sync::Arc::clone(&provider) };
    from_block.search(request()).await;

    let from_if = if condition {
        std::sync::Arc::clone(&provider)
    } else {
        Plain::new()
    };
    from_if.fetch(request()).await;

    let from_match = match key {
        0 => Plain::new(),
        _ => std::sync::Arc::clone(&provider),
    };
    from_match.query(request()).await;

    let mut reassigned = plain;
    reassigned = if condition {
        Plain::new()
    } else {
        std::sync::Arc::clone(&provider)
    };
    reassigned.render(request()).await;

    let mut cleared = std::sync::Arc::clone(&provider);
    cleared = { Plain::new() };
    cleared.get(handle()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
        "provider-method:render",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
    assert!(!error.contains("[provider-method:get]"), "{error}");
}

#[test]
fn direct_wrapper_deref_and_index_provider_receivers_are_tracked() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, providers: Vec<Store>, plain: Plain) {
    std::sync::Arc::clone(&provider).search(request()).await;
    provider.as_ref().fetch(request()).await;
    (*provider).query(request()).await;
    providers[0].render(request()).await;
    provider.clone().as_ref().delete(selector()).await;
    std::sync::Arc::clone(&provider).as_ref().get(handle()).await;

    plain.make_client().upsert(batch()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
        "provider-method:render",
        "provider-method:delete",
        "provider-method:get",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
    assert!(!error.contains("[provider-method:upsert]"), "{error}");
}

#[test]
fn branch_local_tail_bindings_propagate_block_if_and_match_result_shapes() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, condition: bool, key: u8) {
    let from_block = {
        let inner = std::sync::Arc::clone(&provider);
        inner
    };
    from_block.search(request()).await;

    let from_if = if condition {
        let inner = std::sync::Arc::clone(&provider);
        inner
    } else {
        let inner = Plain::new();
        inner
    };
    from_if.fetch(request()).await;

    let from_match = match key {
        0 => {
            let inner = Plain::new();
            inner
        }
        _ => {
            let inner = std::sync::Arc::clone(&provider);
            inner
        }
    };
    from_match.query(request()).await;

    let mut reassigned = plain;
    reassigned = {
        let inner = std::sync::Arc::clone(&provider);
        inner
    };
    reassigned.render(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
        "provider-method:render",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn direct_match_pattern_tail_binding_propagates_result_shape() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(maybe: Option<Store>, plain: Plain) {
    let selected = match maybe {
        Some(inner) => inner,
        None => plain,
    };
    selected.search(request()).await;
}

pub async fn stabilize(provider: Store, plain: Plain, condition: bool) {
    let mut carried = plain;
    while condition {
        let selected = match condition {
            true => carried,
            false => Plain::new(),
        };
        selected.fetch(request()).await;
        carried = std::sync::Arc::clone(&provider);
    }
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in ["provider-method:search", "provider-method:fetch"] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn rust_2024_let_chain_bindings_flow_through_if_and_while_conditions() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, maybe: Option<Store>, condition: bool) {
    if let Some(inner) = maybe && inner.ready() {
        inner.search(request()).await;
    }

    while let Some(inner) = maybe && inner.ready() {
        inner.fetch(request()).await;
        break;
    }

    let mut preserved = std::sync::Arc::clone(&provider);
    if let Some(inner) = maybe && {
        preserved = Plain::new();
        inner.ready()
    } {
        preserved = Plain::new();
    }
    preserved.query(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in [
        "provider-method:search",
        "provider-method:fetch",
        "provider-method:query",
    ] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn while_condition_and_false_let_chain_exits_use_stabilized_state_once() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        r#"
type Store = std::sync::Arc<dyn VectorStore>;
pub async fn run(provider: Store, plain: Plain, maybe: Option<Store>, condition: bool) {
    let mut carried = plain;
    while {
        carried.search(request()).await;
        condition
    } {
        carried = std::sync::Arc::clone(&provider);
    }

    let mut false_exit = plain;
    while let Some(_) = maybe && {
        false_exit = std::sync::Arc::clone(&provider);
        false
    } {
        false_exit = Plain::new();
    }
    false_exit.fetch(request()).await;
}
"#,
    );
    let error = check(temp.path()).unwrap_err().to_string();
    for rule in ["provider-method:search", "provider-method:fetch"] {
        assert_eq!(error.matches(&format!("[{rule}]")).count(), 1, "{error}");
    }
}

#[test]
fn external_test_parent_propagates_test_only_state_to_nested_children() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "#[cfg(test)]\nmod test_parent;\n",
    );
    write(
        &temp.path().join("crates/axon-services/src/test_parent.rs"),
        "mod child;\n",
    );
    write(
        &temp
            .path()
            .join("crates/axon-services/src/test_parent/child.rs"),
        "pub fn test_only(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    check(temp.path()).unwrap();
}

#[test]
fn production_reachability_wins_over_test_only_external_ancestry() {
    let temp = tempdir().unwrap();
    write_surface_fixture(temp.path());
    write(
        &temp.path().join("crates/axon-services/src/lib.rs"),
        "mod production_parent;\n#[cfg(test)]\nmod test_parent;\n",
    );
    for parent in ["production_parent", "test_parent"] {
        write(
            &temp
                .path()
                .join(format!("crates/axon-services/src/{parent}.rs")),
            "#[path = \"shared.rs\"]\nmod child;\n",
        );
    }
    write(
        &temp.path().join("crates/axon-services/src/shared.rs"),
        "pub fn production(runtime: Runtime) { consume(runtime.vector_store); }\n",
    );
    let error = check(temp.path()).unwrap_err().to_string();
    assert!(error.contains("provider-handle:vector_store"), "{error}");
}

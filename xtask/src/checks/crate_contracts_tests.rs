use super::*;
use std::fs;

fn write_crate(root: &Path, name: &str, lib_rs: &str, modules: &[&str], cargo_deps: &str) {
    let src = root.join("crates").join(name).join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), lib_rs).unwrap();
    for module in modules {
        fs::write(src.join(format!("{module}.rs")), "// stub\n").unwrap();
    }
    fs::write(
        root.join("crates").join(name).join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\n\n[dependencies]\n{cargo_deps}\n"),
    )
    .unwrap();
}

#[test]
fn passes_when_modules_and_deps_match_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_crate(
        root,
        "axon-error",
        "pub mod api_error;\npub mod code;\npub mod stage;\npub mod severity;\npub mod retry;\npub mod degradation;\npub mod cooling;\npub mod context;\npub mod conversion;\npub mod testing;\n",
        &[
            "api_error",
            "code",
            "stage",
            "severity",
            "retry",
            "degradation",
            "cooling",
            "context",
            "conversion",
            "testing",
        ],
        "",
    );
    let contracts = [CrateContract {
        name: "axon-error",
        modules: &[
            "api_error",
            "code",
            "stage",
            "severity",
            "retry",
            "degradation",
            "cooling",
            "context",
            "conversion",
            "testing",
        ],
        forbidden_axon_deps: &["axon-api"],
    }];
    let mut violations = Vec::new();
    for contract in &contracts {
        check_modules(root, contract, &mut violations);
        check_forbidden_deps(root, contract, &mut violations);
    }
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn flags_missing_documented_module() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_crate(root, "axon-graph", "pub mod store;\n", &["store"], "");
    let contract = CrateContract {
        name: "axon-graph",
        modules: &["store", "query"],
        forbidden_axon_deps: &[],
    };
    let mut violations = Vec::new();
    check_modules(root, &contract, &mut violations);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("`query.rs` does not exist")),
        "{violations:?}"
    );
}

#[test]
fn flags_module_declared_non_public() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_crate(
        root,
        "axon-retrieval",
        "pub(crate) mod testing;\n",
        &["testing"],
        "",
    );
    let contract = CrateContract {
        name: "axon-retrieval",
        modules: &["testing"],
        forbidden_axon_deps: &[],
    };
    let mut violations = Vec::new();
    check_modules(root, &contract, &mut violations);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("does not declare `pub mod testing;`")),
        "{violations:?}"
    );
}

#[test]
fn flags_forbidden_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_crate(
        root,
        "axon-error",
        "",
        &[],
        "axon-api = { path = \"../axon-api\" }\n",
    );
    let contract = CrateContract {
        name: "axon-error",
        modules: &[],
        forbidden_axon_deps: &["axon-api"],
    };
    let mut violations = Vec::new();
    check_forbidden_deps(root, &contract, &mut violations);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("declares forbidden `axon-api`")),
        "{violations:?}"
    );
}

#[test]
fn ignores_dev_dependencies() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let crate_dir = root.join("crates/axon-graph");
    fs::create_dir_all(crate_dir.join("src")).unwrap();
    fs::write(crate_dir.join("src/lib.rs"), "").unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"axon-graph\"\n\n[dependencies]\n\n[dev-dependencies]\naxon-vectors = { path = \"../axon-vectors\" }\n",
    )
    .unwrap();
    let contract = CrateContract {
        name: "axon-graph",
        modules: &[],
        forbidden_axon_deps: &["axon-vectors"],
    };
    let mut violations = Vec::new();
    check_forbidden_deps(root, &contract, &mut violations);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn flags_forbidden_dependency_under_target_specific_table() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let crate_dir = root.join("crates/axon-graph");
    fs::create_dir_all(crate_dir.join("src")).unwrap();
    fs::write(crate_dir.join("src/lib.rs"), "").unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"axon-graph\"\n\n[dependencies]\n\n[target.'cfg(unix)'.dependencies]\naxon-vectors = { path = \"../axon-vectors\" }\n",
    )
    .unwrap();
    let contract = CrateContract {
        name: "axon-graph",
        modules: &[],
        forbidden_axon_deps: &["axon-vectors"],
    };
    let mut violations = Vec::new();
    check_forbidden_deps(root, &contract, &mut violations);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("declares forbidden `axon-vectors`")),
        "{violations:?}"
    );
}

#[test]
fn every_contract_crate_exists_in_the_real_workspace() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for contract in all_crate_contracts() {
        let crate_dir = root.join("crates").join(contract.name);
        assert!(
            crate_dir.is_dir(),
            "docs/pipeline-unification/crates/{}/README.md has no matching crates/{} directory",
            contract.name,
            contract.name
        );
    }
}

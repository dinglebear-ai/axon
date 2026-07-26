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

#[test]
fn contract_table_exactly_covers_all_23_live_crates() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let expected = LIVE_CRATE_NAMES
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let contracted = all_crate_contracts()
        .map(|contract| contract.name)
        .collect::<std::collections::BTreeSet<_>>();
    let mut inventory_violations = Vec::new();
    let workspace = workspace_crate_members(root, &mut inventory_violations);
    let on_disk = std::fs::read_dir(root.join("crates"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<std::collections::BTreeSet<_>>();
    let on_disk = on_disk
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(contracted, expected, "crate-contract inventory drift");
    assert_eq!(on_disk, expected, "live workspace-crate inventory drift");
    assert_eq!(
        workspace
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        expected,
        "root workspace member inventory drift"
    );
    assert!(inventory_violations.is_empty(), "{inventory_violations:?}");
    assert_eq!(expected.len(), 23);
}

#[test]
fn live_inventory_rejects_duplicate_contract_rows() {
    let rows = LIVE_CRATE_NAMES
        .iter()
        .chain(std::iter::once(&LIVE_CRATE_NAMES[0]))
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let exact = LIVE_CRATE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let mut violations = Vec::new();
    compare_live_inventory(&rows, &exact, &exact, &mut violations);
    assert!(
        violations
            .iter()
            .any(|value| value.contains("duplicate") && value.contains(LIVE_CRATE_NAMES[0])),
        "{violations:?}"
    );
    assert!(
        violations.iter().any(|value| value.contains("row count")),
        "{violations:?}"
    );
}

#[test]
fn live_inventory_rejects_workspace_membership_drift() {
    let exact = LIVE_CRATE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let mut workspace = exact.clone();
    workspace.pop();
    workspace.push("axon-retired".to_owned());
    let mut violations = Vec::new();
    compare_live_inventory(&exact, &workspace, &exact, &mut violations);
    assert!(
        violations
            .iter()
            .any(|value| value.contains("workspace member inventory differs")),
        "{violations:?}"
    );
}

#[test]
fn live_inventory_rejects_on_disk_crate_drift() {
    let exact = LIVE_CRATE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let mut on_disk = exact.clone();
    on_disk.pop();
    let mut violations = Vec::new();
    compare_live_inventory(&exact, &exact, &on_disk, &mut violations);
    assert!(
        violations
            .iter()
            .any(|value| value.contains("on-disk crate inventory differs")),
        "{violations:?}"
    );
}

#[test]
fn live_contracts_do_not_name_retired_crates_as_dependency_rows() {
    for contract in all_crate_contracts() {
        for retired in [
            "axon-vector",
            "axon-crawl",
            "axon-ingest",
            "axon-code-index",
        ] {
            assert!(
                !contract.forbidden_axon_deps.contains(&retired),
                "{} retains deleted-crate row {retired}",
                contract.name
            );
        }
    }
}

#[test]
fn adapter_vertical_dependencies_are_required_and_one_way() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for (name, dependencies) in [
        (
            "axon-adapters",
            "axon-extract = { path = \"../axon-extract\" }\naxon-parse = { path = \"../axon-parse\" }\n",
        ),
        ("axon-extract", ""),
        ("axon-parse", ""),
    ] {
        write_crate(root, name, "", &[], dependencies);
    }

    let mut violations = Vec::new();
    check_adapter_vertical_boundary(root, &mut violations);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn adapter_contract_does_not_reintroduce_stale_vertical_prohibitions() {
    let adapters = all_crate_contracts()
        .find(|contract| contract.name == "axon-adapters")
        .expect("axon-adapters contract");
    for allowed in ADAPTER_VERTICAL_DEPS {
        assert!(
            !adapters.forbidden_axon_deps.contains(allowed),
            "{allowed} is an intentional adapter dependency"
        );
    }
}

#[test]
fn adapter_vertical_boundary_rejects_missing_or_reverse_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_crate(
        root,
        "axon-adapters",
        "",
        &[],
        "axon-extract = { path = \"../axon-extract\" }\n",
    );
    write_crate(
        root,
        "axon-extract",
        "",
        &[],
        "axon-adapters = { path = \"../axon-adapters\" }\n",
    );
    write_crate(root, "axon-parse", "", &[], "");

    let mut violations = Vec::new();
    check_adapter_vertical_boundary(root, &mut violations);
    assert!(
        violations.iter().any(
            |value| value.contains("missing required one-way vertical dependency `axon-parse`")
        ),
        "{violations:?}"
    );
    assert!(
        violations
            .iter()
            .any(|value| value.contains("axon-extract: must not depend on `axon-adapters`")),
        "{violations:?}"
    );
}

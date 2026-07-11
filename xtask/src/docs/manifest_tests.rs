use super::*;
use std::fs;

fn write_family_json(root: &std::path::Path, rel: &str, family: &str, inputs: &[(&str, &str)]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let source_inputs: Vec<Value> = inputs
        .iter()
        .map(|(p, c)| {
            serde_json::json!({"path": p, "kind": "rust_module", "checksum": format!("sha256:{c}")})
        })
        .collect();
    let doc = serde_json::json!({
        "x-axon": {
            "generated_by": format!("cargo xtask schemas {family}"),
            "source_inputs": source_inputs,
        }
    });
    fs::write(path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

#[test]
fn build_groups_by_family_and_dedupes() {
    let dir = tempfile::tempdir().unwrap();
    write_family_json(
        dir.path(),
        "docs/reference/cli/commands.json",
        "cli",
        &[("crates/axon-cli/src/lib.rs", "aaa")],
    );
    write_family_json(
        dir.path(),
        "docs/reference/cli/help.json",
        "cli",
        &[
            ("crates/axon-cli/src/lib.rs", "aaa"),
            ("crates/axon-cli/src/help.rs", "bbb"),
        ],
    );
    let manifest = build(dir.path()).unwrap();
    assert_eq!(manifest.families.len(), 1);
    let cli = &manifest.families[0];
    assert_eq!(cli.family, "cli");
    assert_eq!(cli.source_inputs.len(), 2);
    assert_eq!(cli.generated_by, "cargo xtask docs generate --family cli");
}

#[test]
fn build_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    write_family_json(
        dir.path(),
        "docs/reference/api/schemas.json",
        "api",
        &[("crates/axon-api/src/lib.rs", "111")],
    );
    let first = to_json(&build(dir.path()).unwrap()).unwrap();
    let second = to_json(&build(dir.path()).unwrap()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn build_ignores_non_axon_json() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/reference")).unwrap();
    fs::write(
        dir.path().join("docs/reference/unrelated.json"),
        "{\"a\":1}",
    )
    .unwrap();
    let manifest = build(dir.path()).unwrap();
    assert!(manifest.families.is_empty());
}

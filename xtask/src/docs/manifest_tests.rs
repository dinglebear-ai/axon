use super::*;
use std::fs;

fn write_family_json(root: &Path, rel: &str, family: &str, inputs: &[(&str, &str)]) {
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
fn build_includes_presentation_generator_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join("docs/reference/presentation/tokens.schema.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        serde_json::to_string_pretty(&serde_json::json!({
            "x-axon": {
                "generated_by": "cargo xtask presentation generate",
                "source_inputs": [{
                    "path": "xtask/src/presentation/source.json",
                    "kind": "json_source",
                    "checksum": "sha256:1234"
                }]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let manifest = build(dir.path()).unwrap();
    assert_eq!(manifest.families.len(), 1);
    let presentation = &manifest.families[0];
    assert_eq!(presentation.family, "presentation");
    assert_eq!(
        presentation.generated_by,
        "cargo xtask docs generate --family presentation"
    );
    assert_eq!(
        presentation.source_inputs,
        [SourceInputEntry {
            path: "xtask/src/presentation/source.json".to_owned(),
            kind: "json_source".to_owned(),
            checksum: "sha256:1234".to_owned(),
        }]
    );
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

#[test]
fn refresh_writes_manifest_and_check_rejects_stale_or_missing_output() {
    let dir = tempfile::tempdir().unwrap();
    write_family_json(
        dir.path(),
        "docs/reference/api/schemas.json",
        "api",
        &[("crates/axon-api/src/lib.rs", "111")],
    );

    refresh(dir.path()).unwrap();
    check(dir.path()).unwrap();

    let path = dir.path().join(MANIFEST_PATH);
    fs::write(&path, "{\"families\":[]}\n").unwrap();
    let stale = check(dir.path()).expect_err("stale manifest must fail check");
    assert!(stale.to_string().contains(MANIFEST_PATH), "{stale:#}");

    fs::remove_file(&path).unwrap();
    let missing = check(dir.path()).expect_err("missing manifest must fail check");
    assert!(missing.to_string().contains(MANIFEST_PATH), "{missing:#}");
}

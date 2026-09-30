use super::*;

fn inventory(text: &str) -> Result<Vec<Key>> {
    collect(&[("fixture.rs".to_string(), text.to_string())])
}

#[test]
fn wire_names_follow_serde_and_nested_types() {
    let rows = inventory(
        r#"
        struct RawTomlConfig { server: Server }
        #[serde(deny_unknown_fields, rename_all = "kebab-case")]
        struct Server { max_inputs: Option<usize>, #[serde(rename = "literal")] other_name: bool }
    "#,
    )
    .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["server.literal", "server.max-inputs"]
    );
    assert_eq!(rows[1].rust_type, "Option<usize>");
}

#[test]
fn unresolved_cycle_and_unsupported_serde_fail_closed() {
    for text in [
        "struct RawTomlConfig { broken: RawMissing }",
        "struct RawTomlConfig { recursive: RawTomlConfig }",
        "struct RawTomlConfig { #[serde(flatten)] other: String }",
        "#[serde(rename_all = \"unsupported\")] struct RawTomlConfig { a: String }",
    ] {
        assert!(inventory(text).is_err(), "{text}");
    }
}

#[test]
fn duplicate_wire_names_are_rejected() {
    assert!(inventory(r#"struct RawTomlConfig { #[serde(rename = "same")] a: bool, #[serde(rename = "same")] b: bool }"#).is_err());
}

#[test]
fn real_parser_inventory_contains_literal_keys_not_registry_aliases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let sources = INPUTS[..2]
        .iter()
        .map(|p| {
            (
                p.to_string(),
                std::fs::read_to_string(root.join(p)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let rows = collect(&sources).unwrap();
    assert!(rows.len() > 150);
    for expected in [
        "server.default-collection",
        "providers.llm.synthesis-openai-model",
        "server.projection-batch.max-inputs",
    ] {
        assert!(rows.iter().any(|r| r.key == expected), "missing {expected}");
    }
    assert!(!rows.iter().any(|r| r.key == "server.default_collection"));
}

#[test]
fn generation_is_idempotent_and_check_is_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for input in INPUTS {
        let path = root.join(input);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = if input.ends_with("/raw.rs") {
            "struct RawTomlConfig { enabled: Option<bool> }"
        } else {
            "// fixture input"
        };
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(root.join("docs/reference/config")).unwrap();
    assert!(run(root, true).is_err());
    assert!(!root.join(MD_PATH).exists());
    run(root, false).unwrap();
    let first = std::fs::read(root.join(MD_PATH)).unwrap();
    run(root, false).unwrap();
    assert_eq!(first, std::fs::read(root.join(MD_PATH)).unwrap());
    run(root, true).unwrap();
    std::fs::write(root.join(MD_PATH), "stale").unwrap();
    assert!(run(root, true).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join(MD_PATH)).unwrap(),
        "stale"
    );
}

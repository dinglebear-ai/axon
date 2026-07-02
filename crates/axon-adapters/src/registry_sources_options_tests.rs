use axon_api::source::*;

use super::validate_options;

#[test]
fn accepts_registry_dump_path_option() {
    let mut values = MetadataMap::new();
    values.insert(
        "registry_dump_path".to_string(),
        "/tmp/dump.json".to_string().into(),
    );
    let options = AdapterOptions { values };

    let parsed = validate_options(&options).expect("valid options should parse");

    assert_eq!(parsed.dump_path, std::path::PathBuf::from("/tmp/dump.json"));
    assert!(!parsed.include_all_versions);
}

#[test]
fn accepts_include_all_versions_flag() {
    let mut values = MetadataMap::new();
    values.insert(
        "registry_dump_path".to_string(),
        "/tmp/dump.json".to_string().into(),
    );
    values.insert("include_all_versions".to_string(), true.into());
    let options = AdapterOptions { values };

    let parsed = validate_options(&options).unwrap();

    assert!(parsed.include_all_versions);
}

#[test]
fn rejects_missing_dump_path() {
    let options = AdapterOptions::default();

    let err = validate_options(&options).expect_err("missing dump path should fail");

    assert_eq!(err.code.0, "adapter.registry.option.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}

#[test]
fn rejects_unknown_option_keys() {
    let mut values = MetadataMap::new();
    values.insert(
        "registry_dump_path".to_string(),
        "/tmp/dump.json".to_string().into(),
    );
    values.insert("unexpected_key".to_string(), "value".into());
    let options = AdapterOptions { values };

    let err = validate_options(&options).expect_err("unknown keys should fail");

    assert_eq!(err.code.0, "adapter.registry.option.invalid");
}

#[test]
fn rejects_non_boolean_include_all_versions() {
    let mut values = MetadataMap::new();
    values.insert(
        "registry_dump_path".to_string(),
        "/tmp/dump.json".to_string().into(),
    );
    values.insert("include_all_versions".to_string(), "yes".into());
    let options = AdapterOptions { values };

    let err = validate_options(&options).expect_err("non-boolean flag should fail");

    assert_eq!(err.code.0, "adapter.registry.option.invalid");
}

use super::RegistryDump;

fn valid_json() -> &'static str {
    r##"{
        "registry": "pypi",
        "package": "requests",
        "description": "Python HTTP for Humans.",
        "homepage": "https://requests.readthedocs.io",
        "license": "Apache-2.0",
        "author": "Kenneth Reitz",
        "keywords": ["http", "requests"],
        "versions": [
            {
                "version": "2.31.0",
                "readme": "# Requests\n\nHTTP library.",
                "description": "Python HTTP for Humans.",
                "published_at": "2023-05-22T00:00:00Z",
                "is_latest": true
            }
        ]
    }"##
}

#[test]
fn parses_valid_dump_with_all_fields() {
    let dump = RegistryDump::parse(valid_json()).expect("valid dump should parse");

    assert_eq!(dump.registry, "pypi");
    assert_eq!(dump.package, "requests");
    assert_eq!(dump.description.as_deref(), Some("Python HTTP for Humans."));
    assert_eq!(
        dump.keywords,
        vec!["http".to_string(), "requests".to_string()]
    );
    assert_eq!(dump.versions.len(), 1);
    assert_eq!(dump.versions[0].version, "2.31.0");
    assert!(dump.versions[0].is_latest);
}

#[test]
fn parses_minimal_valid_dump_with_only_required_fields() {
    let json = r#"{
        "registry": "npm",
        "package": "tiny",
        "versions": [{"version": "1.0.0"}]
    }"#;

    let dump = RegistryDump::parse(json).expect("minimal dump should parse");

    assert_eq!(dump.registry, "npm");
    assert_eq!(dump.package, "tiny");
    assert!(dump.description.is_none());
    assert!(dump.keywords.is_empty());
    assert_eq!(dump.versions.len(), 1);
    assert!(!dump.versions[0].is_latest);
    assert!(dump.versions[0].readme.is_none());
}

#[test]
fn rejects_malformed_json() {
    let err = RegistryDump::parse("not json at all").expect_err("malformed JSON should fail");

    assert_eq!(err.code.0, "adapter.registry.dump_malformed");
    assert_eq!(err.stage, axon_error::ErrorStage::Discovering);
}

#[test]
fn rejects_truncated_json() {
    let err = RegistryDump::parse(r#"{"registry": "npm", "package": "#)
        .expect_err("truncated JSON should fail");

    assert_eq!(err.code.0, "adapter.registry.dump_malformed");
}

#[test]
fn rejects_empty_string() {
    let err = RegistryDump::parse("").expect_err("empty input should fail");

    assert_eq!(err.code.0, "adapter.registry.dump_malformed");
}

#[test]
fn rejects_dump_with_no_versions() {
    let json = r#"{"registry": "npm", "package": "tiny", "versions": []}"#;

    let err = RegistryDump::parse(json).expect_err("empty versions array should fail validation");

    assert_eq!(err.code.0, "adapter.registry.dump_invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Discovering);
}

#[test]
fn rejects_dump_with_empty_registry_name() {
    let json = r#"{"registry": "", "package": "tiny", "versions": [{"version": "1.0.0"}]}"#;

    let err = RegistryDump::parse(json).expect_err("empty registry name should fail validation");

    assert_eq!(err.code.0, "adapter.registry.dump_invalid");
}

#[test]
fn rejects_dump_with_empty_package_name() {
    let json = r#"{"registry": "npm", "package": "  ", "versions": [{"version": "1.0.0"}]}"#;

    let err = RegistryDump::parse(json).expect_err("blank package name should fail validation");

    assert_eq!(err.code.0, "adapter.registry.dump_invalid");
}

#[test]
fn rejects_version_entry_with_empty_version_string() {
    let json = r#"{"registry": "npm", "package": "tiny", "versions": [{"version": ""}]}"#;

    let err = RegistryDump::parse(json).expect_err("empty version string should fail validation");

    assert_eq!(err.code.0, "adapter.registry.dump_invalid");
}

#[test]
fn rejects_dump_missing_required_field() {
    let json = r#"{"package": "tiny", "versions": [{"version": "1.0.0"}]}"#;

    let err = RegistryDump::parse(json).expect_err("missing registry field should fail parsing");

    assert_eq!(err.code.0, "adapter.registry.dump_malformed");
}

#[test]
fn latest_version_prefers_flagged_entry() {
    let json = r#"{
        "registry": "npm",
        "package": "lodash",
        "versions": [
            {"version": "4.17.20", "is_latest": false},
            {"version": "4.17.21", "is_latest": true},
            {"version": "4.17.19", "is_latest": false}
        ]
    }"#;

    let dump = RegistryDump::parse(json).unwrap();

    assert_eq!(dump.latest_version().unwrap().version, "4.17.21");
}

#[test]
fn latest_version_falls_back_to_last_entry_when_none_flagged() {
    let json = r#"{
        "registry": "npm",
        "package": "lodash",
        "versions": [
            {"version": "1.0.0"},
            {"version": "2.0.0"}
        ]
    }"#;

    let dump = RegistryDump::parse(json).unwrap();

    assert_eq!(dump.latest_version().unwrap().version, "2.0.0");
}

#[test]
fn version_lookup_finds_matching_entry() {
    let dump = RegistryDump::parse(valid_json()).unwrap();

    assert!(dump.version("2.31.0").is_some());
    assert!(dump.version("9.9.9").is_none());
}

#[test]
fn load_reports_unreadable_path_error() {
    let err = RegistryDump::load(std::path::Path::new("/nonexistent/dump.json"))
        .expect_err("missing file should fail to load");

    assert_eq!(err.code.0, "adapter.registry.dump_unreadable");
    assert_eq!(err.stage, axon_error::ErrorStage::Discovering);
}

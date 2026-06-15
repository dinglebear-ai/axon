use super::*;

#[test]
fn cargo_version_extracted() {
    let toml = r#"
[workspace]
members = ["xtask"]

[package]
name = "axon"
version = "5.6.1"
edition = "2024"
"#;
    assert_eq!(read_cargo_version(toml), Some("5.6.1".to_owned()));
}

#[test]
fn cargo_version_not_in_workspace_section() {
    let toml = r#"
[workspace]
members = ["xtask"]
# version = "0.0.0"  -- workspace key, not package

[package]
name = "axon"
version = "1.2.3"
"#;
    assert_eq!(read_cargo_version(toml), Some("1.2.3".to_owned()));
}

#[test]
fn readme_version_ok() {
    let readme = "# Axon\n\nVersion: 5.6.1\n\nSome content.";
    assert!(check_readme(readme, "5.6.1").is_ok());
}

#[test]
fn readme_version_missing() {
    let readme = "# Axon\n\nVersion: 5.6.0\n";
    assert!(check_readme(readme, "5.6.1").is_err());
}

#[test]
fn changelog_heading_ok() {
    let changelog = "# Changelog\n\n## [5.6.1] - 2026-06-01\n\n### Fixed\n- stuff\n";
    assert!(check_changelog(changelog, "5.6.1").is_ok());
}

#[test]
fn changelog_heading_missing() {
    let changelog = "# Changelog\n\n## [5.6.0] - 2026-05-01\n";
    assert!(check_changelog(changelog, "5.6.1").is_err());
}

#[test]
fn json_version_ok() {
    let json = "{\n  \"name\": \"axon-web\",\n  \"version\": \"5.6.1\",\n}";
    assert!(check_json_version(json, "5.6.1", "apps/web/package.json").is_ok());
}

#[test]
fn json_version_nested_ok() {
    // OpenAPI carries the version nested under info; a substring match still
    // finds it regardless of nesting/indentation.
    let json = "{\n  \"openapi\": \"3.1.0\",\n  \"info\": {\n    \"version\": \"5.6.1\"\n  }\n}";
    assert!(check_json_version(json, "5.6.1", "apps/web/openapi/axon.json").is_ok());
}

#[test]
fn json_version_compact_ok() {
    // Compact JSON with no space after the colon must still match.
    let json = "{\"name\":\"axon-web\",\"version\":\"5.6.1\"}";
    assert!(check_json_version(json, "5.6.1", "apps/web/package.json").is_ok());
}

#[test]
fn json_version_mismatch_is_error() {
    let json = "{\n  \"version\": \"5.6.0\"\n}";
    assert!(check_json_version(json, "5.6.1", "apps/web/package.json").is_err());
}

#[test]
fn plugin_json_no_version_ok() {
    let json = r#"{"name": "axon", "description": "..."}"#;
    assert!(check_plugin_json(json, "plugin.json").is_ok());
}

#[test]
fn plugin_json_with_version_is_error() {
    let json = r#"{"name": "axon", "version": "5.6.1"}"#;
    assert!(check_plugin_json(json, "plugin.json").is_err());
}

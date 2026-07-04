use axon_api::source::*;

use crate::registry_sources::dump::RegistryDump;
use crate::registry_sources_test_support::{source_plan, valid_dump_json, write_dump};

use super::{package_markdown, package_metadata, registry_document_id};

#[test]
fn package_markdown_renders_header_metadata_and_readme() {
    let dump = RegistryDump::parse(valid_dump_json()).unwrap();
    let version = dump.version("4.17.21").unwrap();

    let markdown = package_markdown(&dump, version);

    assert!(markdown.starts_with("# lodash@4.17.21\n"));
    assert!(markdown.contains("A modern JavaScript utility library."));
    assert!(markdown.contains("**Author:** jdd"));
    assert!(markdown.contains("**License:** MIT"));
    assert!(markdown.contains("**Homepage:** https://lodash.com"));
    assert!(markdown.contains("**Keywords:** array, util"));
    assert!(markdown.contains("**Registry:** npm"));
    assert!(markdown.contains("## README"));
    assert!(markdown.contains("A modern JavaScript utility library."));
}

#[test]
fn package_markdown_omits_absent_optional_sections() {
    let json = r#"{"registry": "npm", "package": "tiny", "versions": [{"version": "1.0.0"}]}"#;
    let dump = RegistryDump::parse(json).unwrap();
    let version = dump.version("1.0.0").unwrap();

    let markdown = package_markdown(&dump, version);

    assert!(markdown.starts_with("# tiny@1.0.0\n"));
    assert!(!markdown.contains("**Author:**"));
    assert!(!markdown.contains("**License:**"));
    assert!(!markdown.contains("**Homepage:**"));
    assert!(!markdown.contains("**Keywords:**"));
    assert!(!markdown.contains("## README"));
    assert!(markdown.contains("**Registry:** npm"));
}

#[tokio::test]
async fn package_metadata_stamps_expected_pkg_fields() {
    let dump = RegistryDump::parse(valid_dump_json()).unwrap();
    let version = dump.version("4.17.21").unwrap();
    let dump_path = write_dump(valid_dump_json());
    let plan = source_plan(dump_path, SourceScope::Package);

    let metadata = package_metadata(&plan, &dump, version);

    assert_eq!(
        metadata.get("source_family").and_then(|v| v.as_str()),
        Some("registry")
    );
    assert_eq!(
        metadata.get("pkg_registry").and_then(|v| v.as_str()),
        Some("npm")
    );
    assert_eq!(
        metadata.get("pkg_name").and_then(|v| v.as_str()),
        Some("lodash")
    );
    assert_eq!(
        metadata.get("pkg_version").and_then(|v| v.as_str()),
        Some("4.17.21")
    );
    assert_eq!(
        metadata.get("pkg_license").and_then(|v| v.as_str()),
        Some("MIT")
    );
    assert_eq!(
        metadata.get("pkg_author").and_then(|v| v.as_str()),
        Some("jdd")
    );
    assert_eq!(
        metadata.get("pkg_homepage").and_then(|v| v.as_str()),
        Some("https://lodash.com")
    );
    assert!(metadata.get("pkg_keywords").is_some());
}

#[tokio::test]
async fn package_metadata_omits_absent_optional_pkg_fields() {
    let json = r#"{"registry": "npm", "package": "tiny", "versions": [{"version": "1.0.0"}]}"#;
    let dump = RegistryDump::parse(json).unwrap();
    let version = dump.version("1.0.0").unwrap();
    let dump_path = write_dump(json);
    let plan = source_plan(dump_path, SourceScope::Package);

    let metadata = package_metadata(&plan, &dump, version);

    assert!(metadata.get("pkg_license").is_none());
    assert!(metadata.get("pkg_author").is_none());
    assert!(metadata.get("pkg_keywords").is_none());
    assert!(metadata.get("pkg_homepage").is_none());
}

#[test]
fn registry_document_id_is_stable_and_namespaced() {
    let source_id = SourceId::from("src_registry_test");
    let key = SourceItemKey::from("versions/4.17.21");

    let first = registry_document_id(&source_id, &key);
    let second = registry_document_id(&source_id, &key);

    assert_eq!(first, second);
    assert!(first.0.starts_with("doc_registry_"));
}

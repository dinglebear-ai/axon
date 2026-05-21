use super::build_extra;

#[test]
fn pypi_extra_fields() {
    let extra = build_extra(
        "requests",
        "2.31.0",
        "Apache-2.0",
        "Kenneth Reitz",
        &["http", "requests", "web"],
        "https://requests.readthedocs.io",
        ">=3.7",
    );
    assert_eq!(extra["pkg_registry"], "pypi");
    assert_eq!(extra["pkg_name"], "requests");
    assert_eq!(extra["pkg_language"], "python");
    assert_eq!(extra["pkg_license"], "Apache-2.0");
    assert_eq!(extra["pypi_requires_python"], ">=3.7");
}

#[test]
fn pypi_extra_empty_optional_fields_absent() {
    let extra = build_extra("tiny", "1.0.0", "", "", &[], "", "");
    assert!(extra.get("pkg_license").is_none());
    assert!(extra.get("pkg_author").is_none());
    assert!(extra.get("pkg_keywords").is_none());
    assert!(extra.get("pkg_homepage").is_none());
    assert!(extra.get("pypi_requires_python").is_none());
}

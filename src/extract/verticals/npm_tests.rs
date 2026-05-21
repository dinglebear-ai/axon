use super::build_extra;

#[test]
fn npm_extra_fields() {
    let extra = build_extra(
        "lodash",
        "4.17.21",
        "MIT",
        "jdd",
        &["array", "util"],
        "https://lodash.com",
        Some("https://github.com/lodash/lodash"),
    );
    assert_eq!(extra["pkg_registry"], "npm");
    assert_eq!(extra["pkg_name"], "lodash");
    assert_eq!(extra["pkg_language"], "javascript");
    assert_eq!(extra["pkg_license"], "MIT");
}

#[test]
fn npm_extra_empty_optional_fields_absent() {
    let extra = build_extra("tiny", "1.0.0", "", "", &[], "", None);
    assert!(extra.get("pkg_license").is_none());
    assert!(extra.get("pkg_repo_url").is_none());
}

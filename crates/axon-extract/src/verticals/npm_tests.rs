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
    assert_eq!(extra["pkg_version"], "4.17.21");
    assert_eq!(extra["pkg_language"], "javascript");
    assert_eq!(extra["pkg_license"], "MIT");
    assert_eq!(extra["pkg_author"], "jdd");
    assert_eq!(extra["pkg_keywords"], serde_json::json!(["array", "util"]));
    assert_eq!(extra["pkg_homepage"], "https://lodash.com");
    assert_eq!(extra["pkg_repo_url"], "https://github.com/lodash/lodash");
}

#[test]
fn npm_extra_empty_optional_fields_absent() {
    let extra = build_extra("tiny", "1.0.0", "", "", &[], "", None);
    assert_eq!(extra["pkg_registry"], "npm");
    assert_eq!(extra["pkg_name"], "tiny");
    assert_eq!(extra["pkg_version"], "1.0.0");
    assert!(extra.get("pkg_license").is_none());
    assert!(extra.get("pkg_author").is_none());
    assert!(extra.get("pkg_keywords").is_none());
    assert!(extra.get("pkg_homepage").is_none());
    assert!(extra.get("pkg_repo_url").is_none());
}

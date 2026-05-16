use super::is_excluded_url_path;

#[test]
fn excludes_first_segment_relative_locale_paths() {
    let prefixes = vec!["/fr".to_string(), "/ja".to_string()];
    assert!(is_excluded_url_path(
        "https://example.com/docs/fr/settings",
        &prefixes
    ));
    assert!(is_excluded_url_path(
        "https://example.com/docs/ja-jp/settings",
        &prefixes
    ));
    assert!(!is_excluded_url_path(
        "https://example.com/docs/javascript",
        &prefixes
    ));
}

#[test]
fn excludes_case_insensitive() {
    let prefixes = vec!["/zh-tw".to_string(), "/ZH-CN".to_string()];
    assert!(is_excluded_url_path(
        "https://example.com/zh-TW/mcp",
        &prefixes
    ));
    assert!(is_excluded_url_path(
        "https://example.com/zh-cn/mcp",
        &prefixes
    ));
}

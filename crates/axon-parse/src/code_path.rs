//! Shared path classification used by repository acquisition and preparation.

pub fn is_test_path(path: Option<&str>) -> bool {
    let Some(path) = path else { return false };
    let lower = path.to_ascii_lowercase();
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("_test.")
        || lower.contains("_tests.")
        || lower.contains(".test.")
        || lower.contains(".tests.")
        || lower.contains(".spec.")
        || lower.starts_with("test_")
        || lower.contains("/test_")
}

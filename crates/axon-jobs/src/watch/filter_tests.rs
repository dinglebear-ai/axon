use super::*;

#[test]
fn whitespace_only_change_same_hash() {
    let a = content_hash(&normalize_markdown("# T\n\nBody\n"));
    let b = content_hash(&normalize_markdown("# T  \r\n\r\n\r\nBody   \n\n"));
    assert_eq!(a, b);
}

#[test]
fn real_change_differs() {
    assert_ne!(
        content_hash(&normalize_markdown("# T\n\nBody\n")),
        content_hash(&normalize_markdown("# T\n\nBody edited\n"))
    );
}

#[test]
fn hash_is_64_hex() {
    let h = content_hash("x");
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn apply_ignore_strips_matching_lines() {
    let patterns = compile_patterns(&["^Last updated:".to_string()]).unwrap();
    let filtered = apply_ignore("Title\nLast updated: 2026\nBody", &patterns);
    assert_eq!(filtered, "Title\nBody");
}

#[test]
fn compile_patterns_rejects_bad_regex() {
    assert!(compile_patterns(&["(".to_string()]).is_err());
}

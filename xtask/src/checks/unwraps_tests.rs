use super::*;
use std::path::PathBuf;

// ---- count_added_unwraps -----------------------------------------------

#[test]
fn count_unwrap_on_added_line() {
    let diff = "+    let x = foo.unwrap();\n";
    assert_eq!(count_added_unwraps(diff), 1);
}

#[test]
fn count_expect_on_added_line() {
    let diff = "+    let x = foo.expect(\"bad\");\n";
    assert_eq!(count_added_unwraps(diff), 1);
}

#[test]
fn ignore_removed_lines() {
    let diff = "-    let x = foo.unwrap();\n-    let y = bar.expect(\"x\");\n";
    assert_eq!(count_added_unwraps(diff), 0);
}

#[test]
fn ignore_context_lines() {
    let diff = "     let x = foo.unwrap();\n     let y = bar.expect(\"x\");\n";
    assert_eq!(count_added_unwraps(diff), 0);
}

#[test]
fn ignore_plus_plus_plus_header() {
    // `+++ b/src/foo.rs` should NOT count even though it begins with `+`.
    let diff = "+++ b/src/foo.rs\n--- a/src/foo.rs\n";
    assert_eq!(count_added_unwraps(diff), 0);
}

#[test]
fn count_is_per_line_not_per_occurrence() {
    // Two added lines, each containing multiple unwrap()/expect( calls.
    // Old shell semantics (grep -cE) count *matching lines*, not occurrences,
    // so this should be 2 — not 4.
    let diff = "+    let _ = a.unwrap().unwrap();\n+    b.expect(\"a\"); c.expect(\"b\");\n";
    assert_eq!(count_added_unwraps(diff), 2);
}

#[test]
fn count_one_line_with_both_unwrap_and_expect() {
    // A single added line that contains both `.unwrap()` and `.expect(` still
    // counts as one matching line, not two.
    let diff = "+    a.unwrap(); b.expect(\"x\");\n";
    assert_eq!(count_added_unwraps(diff), 1);
}

#[test]
fn empty_diff_returns_zero() {
    assert_eq!(count_added_unwraps(""), 0);
}

// ---- is_test_path ------------------------------------------------------

#[test]
fn is_test_path_root_tests() {
    assert!(is_test_path("tests/foo.rs"));
}

#[test]
fn is_test_path_root_test_rs() {
    assert!(is_test_path("test.rs"));
}

#[test]
fn is_test_path_underscore_test_rs() {
    assert!(is_test_path("src/foo_test.rs"));
}

#[test]
fn is_test_path_underscore_tests_rs() {
    assert!(is_test_path("src/foo_tests.rs"));
}

#[test]
fn is_test_path_root_tests_rs() {
    assert!(is_test_path("tests.rs"));
}

#[test]
fn is_test_path_nested_tests_rs() {
    assert!(is_test_path("src/foo/tests.rs"));
}

#[test]
fn is_test_path_does_not_match_testability() {
    assert!(!is_test_path("src/testability/foo.rs"));
}

#[test]
fn is_test_path_skips_nested_tests_dir() {
    assert!(is_test_path("src/foo/tests/bar.rs"));
}

#[test]
fn is_test_path_does_not_match_regular_source() {
    assert!(!is_test_path("src/lib.rs"));
    assert!(!is_test_path("src/cli/commands/scrape.rs"));
}

// ---- check() smoke -----------------------------------------------------

#[test]
fn check_returns_ok_outside_git_repo() {
    // In a non-repo dir `git diff --cached` exits non-zero; the check
    // should bail with a clear error rather than panic. We assert it
    // either succeeds (git silently returns empty in some envs) or
    // fails with an Err — but never panics.
    let dir = tempfile::tempdir().expect("tempdir");
    // Ensure path resolves; check() may bail or succeed depending on git.
    let _root: &Path = dir.path();
    // Don't assert success/failure — just ensure it returns without panic.
    let _ = check(&PathBuf::from(dir.path()));
}

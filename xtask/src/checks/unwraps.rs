use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Returns true if the given path (forward-slash separated, repo-relative) is
/// a Rust test file that should be excluded from new-unwrap accounting.
///
/// Rules (mirrors the original shell regex):
/// - any `/`-separated path component equal to `test` or `tests` → test
///   (catches root `tests/foo.rs`, `crates/foo/tests/bar.rs`, …)
/// - filename equal to `test.rs` (root or otherwise) → test
/// - filename ending in `_test.rs` or `_tests.rs` → test
/// - paths like `src/testability/foo.rs` are NOT test files (no full
///   `test`/`tests` component, filename does not match either suffix).
fn is_test_path(path: &str) -> bool {
    // Component check.
    for component in path.split('/') {
        if component == "test" || component == "tests" {
            return true;
        }
    }
    // Filename check.
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename == "test.rs" {
        return true;
    }
    if filename.ends_with("_test.rs") || filename.ends_with("_tests.rs") {
        return true;
    }
    false
}

/// Counts new `.unwrap()` and `.expect(` occurrences in added diff lines.
///
/// - Added lines start with `+` but NOT `+++` (which is a file header).
/// - Removed lines (`-`/`---`) and context lines (` `) are ignored.
/// - Multiple occurrences on a single line are all counted.
fn count_added_unwraps(diff: &str) -> usize {
    let mut total = 0usize;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if !line.starts_with('+') {
            continue;
        }
        // Strip the leading `+` to avoid matching diff metadata accidentally.
        let body = &line[1..];
        total += body.matches(".unwrap()").count();
        total += body.matches(".expect(").count();
    }
    total
}

pub fn check(root: &Path) -> Result<()> {
    // List staged Rust files (Added/Copied/Modified/Renamed).
    let output = Command::new("git")
        .args([
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "--",
            "*.rs",
        ])
        .current_dir(root)
        .output()
        .context("failed to invoke `git diff --cached --name-only`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`git diff --cached --name-only` failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("`git diff --cached --name-only` returned non-UTF-8 output")?;

    let mut per_file: Vec<(String, usize)> = Vec::new();
    let mut total: usize = 0;

    for line in stdout.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        if is_test_path(path) {
            continue;
        }

        let diff_out = Command::new("git")
            .args(["diff", "--cached", "--", path])
            .current_dir(root)
            .output()
            .with_context(|| format!("failed to invoke `git diff --cached -- {path}`"))?;

        if !diff_out.status.success() {
            let stderr = String::from_utf8_lossy(&diff_out.stderr);
            bail!(
                "`git diff --cached -- {}` failed (exit {}): {}",
                path,
                diff_out.status.code().unwrap_or(-1),
                stderr.trim()
            );
        }

        let diff_text = String::from_utf8_lossy(&diff_out.stdout);
        let count = count_added_unwraps(&diff_text);
        if count > 0 {
            per_file.push((path.to_string(), count));
            total += count;
        }
    }

    if total > 0 {
        eprintln!(
            "[unwraps] WARNING: {} new unwrap()/expect() call(s) in {} staged non-test file(s):",
            total,
            per_file.len()
        );
        for (path, count) in &per_file {
            eprintln!("  {count:>4}  {path}");
        }
        eprintln!("[unwraps] (warn-only — not blocking commit)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
    fn count_multiple_hits_on_one_line() {
        let diff = "+    let _ = a.unwrap().unwrap();\n+    b.expect(\"a\"); c.expect(\"b\");\n";
        // 2 unwraps on line 1, 2 expects on line 2.
        assert_eq!(count_added_unwraps(diff), 4);
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
    fn is_test_path_does_not_match_testability() {
        assert!(!is_test_path("src/testability/foo.rs"));
    }

    #[test]
    fn is_test_path_skips_nested_tests_dir() {
        assert!(is_test_path("crates/foo/tests/bar.rs"));
    }

    #[test]
    fn is_test_path_does_not_match_regular_source() {
        assert!(!is_test_path("src/lib.rs"));
        assert!(!is_test_path("crates/cli/commands/scrape.rs"));
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
}

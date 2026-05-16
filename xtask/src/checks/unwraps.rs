use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Returns true if the given path (forward-slash separated, repo-relative) is
/// a Rust test file that should be excluded from new-unwrap accounting.
///
/// Rules (mirrors the original shell regex `(^|/)tests?(/|\.rs$)` plus
/// `_test(s)?\.rs$`):
/// - any `/`-separated path component equal to `test` or `tests` → test
///   (catches root `tests/foo.rs`, `src/foo/tests/bar.rs`, …)
/// - filename equal to `test.rs` or `tests.rs` (root or otherwise) → test
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
    if filename == "test.rs" || filename == "tests.rs" {
        return true;
    }
    if filename.ends_with("_test.rs") || filename.ends_with("_tests.rs") {
        return true;
    }
    false
}

/// Counts the number of *added diff lines* that contain at least one
/// `.unwrap()` or `.expect(` call. Mirrors the original shell `grep -cE`
/// semantics — chained calls on a single line count once, not twice — so
/// historical warning totals stay comparable.
///
/// - Added lines start with `+` but NOT `+++` (which is a file header).
/// - Removed lines (`-`/`---`) and context lines (` `) are ignored.
/// - A line containing both `.unwrap()` and `.expect(` counts as one match.
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
        if body.contains(".unwrap()") || body.contains(".expect(") {
            total += 1;
        }
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
#[path = "unwraps_tests.rs"]
mod tests;

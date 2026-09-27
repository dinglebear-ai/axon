use anyhow::{Result, bail};
use std::collections::BTreeSet;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

// Other worktrees and immutable review snapshots do not belong to this checkout.
// See AGENTS.md. Never follow aliases while discovering instruction directories.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".full-review",
    ".full-review-archive",
    "node_modules",
    "target",
    ".cache",
    ".next",
    ".worktrees",
];
const CANONICAL: &str = "AGENTS.md";
const ALIASES: &[&str] = &["CLAUDE.md", "GEMINI.md"];

fn is_excluded_dir(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry.file_type().is_dir()
        && entry.file_name().to_str().is_some_and(|name| {
            let cargo_cache = matches!(name, "registry" | "git")
                && entry
                    .path()
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|parent| parent == ".cargo");
            SKIP_DIRS.contains(&name) || name.starts_with(".full-review-archive-") || cargo_cache
        })
}

/// Shared by the full-tree check and the crate-structure contract.
pub(super) fn validate_dir(dir: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let canonical = dir.join(CANONICAL);
    match canonical.symlink_metadata() {
        Ok(meta) if meta.file_type().is_file() => {}
        _ => errors.push(format!(
            "{} must be a regular canonical file, not a symlink",
            canonical.display()
        )),
    }
    for alias in ALIASES {
        let path = dir.join(alias);
        match std::fs::read_link(&path) {
            Ok(target) if target == Path::new(CANONICAL) => {}
            _ => errors.push(format!(
                "{} must be a direct relative symlink to {CANONICAL}",
                path.display()
            )),
        }
    }
    errors
}

pub fn check(root: &Path) -> Result<()> {
    // Always require root guidance. Discover any instruction name, including
    // symlinks, so the old reversed layout and orphan aliases cannot be skipped.
    let mut directories = BTreeSet::from([root.to_path_buf()]);
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_excluded_dir(entry))
    {
        let entry = entry?;
        let name = entry.file_name().to_str();
        if name == Some(CANONICAL) || name.is_some_and(|name| ALIASES.contains(&name)) {
            if let Some(dir) = entry.path().parent() {
                directories.insert(dir.to_path_buf());
            }
        }
    }

    let mut failures = 0;
    for dir in &directories {
        for error in validate_dir(dir) {
            println!("[claude-symlinks] {error}");
            failures += 1;
        }
    }
    if failures > 0 {
        println!(
            "[claude-symlinks] Preserve guidance in AGENTS.md, then create CLAUDE.md and GEMINI.md as relative symlinks to AGENTS.md."
        );
        bail!("{failures} claude-symlinks failure(s)");
    }
    println!(
        "[claude-symlinks] OK — {} canonical AGENTS.md files with direct CLAUDE.md + GEMINI.md aliases",
        directories.len()
    );
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "claude_symlinks_tests.rs"]
mod tests;

use anyhow::{Result, bail};
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

// `.worktrees` is the documented home for sibling worktrees in this repo
// (see CLAUDE.md). Recursing into it would surface symlink failures that
// belong to other branch checkouts, not the current one.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cache",
    ".next",
    ".worktrees",
];
const TARGETS: &[&str] = &["AGENTS.md", "GEMINI.md"];

fn is_excluded_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    if entry.depth() == 0 {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|name| SKIP_DIRS.contains(&name))
        .unwrap_or(false)
}

fn rel_dir_display(root: &Path, dir: &Path) -> String {
    if dir == root {
        return ".".to_string();
    }
    match dir.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => dir.to_string_lossy().into_owned(),
    }
}

pub fn check(root: &Path) -> Result<()> {
    let mut failures = 0usize;

    let walker = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e));

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != "CLAUDE.md" {
            continue;
        }
        let dir = match entry.path().parent() {
            Some(p) => p,
            None => continue,
        };
        let rel_dir = rel_dir_display(root, dir);

        for target in TARGETS {
            let link = dir.join(target);
            match link.symlink_metadata() {
                Err(_) => {
                    println!(
                        "[claude-symlinks] MISSING: {}/{} (should be a symlink to CLAUDE.md)",
                        rel_dir, target
                    );
                    failures += 1;
                }
                Ok(meta) => {
                    if !meta.file_type().is_symlink() {
                        println!(
                            "[claude-symlinks] NOT A SYMLINK: {}/{} (must be: ln -sf CLAUDE.md {})",
                            rel_dir, target, target
                        );
                        failures += 1;
                    } else {
                        let dest = std::fs::read_link(&link)?;
                        let dest_str = dest.to_string_lossy();
                        if dest_str != "CLAUDE.md" {
                            println!(
                                "[claude-symlinks] WRONG TARGET: {}/{} -> {} (expected -> CLAUDE.md)",
                                rel_dir, target, dest_str
                            );
                            failures += 1;
                        }
                    }
                }
            }
        }
    }

    if failures > 0 {
        println!();
        println!(
            "[claude-symlinks] Fix with: ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md"
        );
        println!("[claude-symlinks] Run from each directory listed above.");
        bail!("{} claude-symlinks failure(s)", failures);
    }

    println!(
        "[claude-symlinks] OK — all CLAUDE.md files have valid AGENTS.md + GEMINI.md symlinks"
    );
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "claude_symlinks_tests.rs"]
mod tests;

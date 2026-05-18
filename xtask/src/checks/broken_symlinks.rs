use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".cache",
    ".next",
    ".worktrees",
];

fn is_skipped_dir(entry: &DirEntry) -> bool {
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

pub fn check(root: &Path) -> Result<()> {
    let mut broken: Vec<(PathBuf, PathBuf)> = Vec::new();

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped_dir(e));

    for entry in walker {
        let entry = entry?;
        let meta = match entry.path().symlink_metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        if entry.path().exists() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();
        let target =
            std::fs::read_link(entry.path()).unwrap_or_else(|_| PathBuf::from("<unknown>"));
        broken.push((rel, target));
    }

    broken.sort();

    if !broken.is_empty() {
        eprintln!("ERROR: broken symlinks detected (target does not exist):");
        for (path, target) in &broken {
            eprintln!("  {} -> {}", path.display(), target.display());
        }
        eprintln!();
        eprintln!("Either restore the target or remove the symlink.");
        bail!("found {} broken symlink(s)", broken.len());
    }

    println!("OK: no broken symlinks found.");
    Ok(())
}

#[cfg(test)]
#[path = "broken_symlinks_tests.rs"]
mod tests;

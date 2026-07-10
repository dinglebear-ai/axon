//! Repo-wide relative-link check for `docs check`.
//!
//! `cargo xtask check-doc-links` only walks `docs/reference/**`. The docs
//! generator contract calls for link checking across all repo markdown, so
//! this reuses the same link-extraction/resolution primitives
//! (`crate::checks::doc_links`) over the whole worktree instead.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::checks::doc_links::extract_relative_link_targets;

/// Directory names skipped entirely while walking for markdown files.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".worktrees",
    "target",
    "node_modules",
    "dist",
    "build",
    ".venv",
    "vendor",
    ".cache",
];

pub fn check_repo_wide(root: &Path) -> Result<()> {
    let mut broken = Vec::new();
    let mut checked = 0usize;
    let walker = WalkDir::new(root).into_iter().filter_entry(|entry| {
        if !entry.file_type().is_dir() {
            return true;
        }
        entry
            .file_name()
            .to_str()
            .is_none_or(|name| !SKIP_DIRS.contains(&name))
    });
    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        checked += 1;
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let dir = path.parent().unwrap_or(root);
        for target in extract_relative_link_targets(&content) {
            if link_target_exists(dir, &target) {
                continue;
            }
            broken.push((rel(root, path), target));
        }
    }
    if !broken.is_empty() {
        let mut msg = format!(
            "check-doc-links (repo-wide): {} broken link(s):\n",
            broken.len()
        );
        for (source, target) in &broken {
            msg.push_str(&format!("  {source} -> {target}\n"));
        }
        bail!(msg);
    }
    println!("check-doc-links (repo-wide): {checked} markdown file(s), no broken relative links.");
    Ok(())
}

fn link_target_exists(dir: &Path, target: &str) -> bool {
    let path_part = target
        .split('#')
        .next()
        .unwrap_or(target)
        .split('?')
        .next()
        .unwrap_or(target);
    if path_part.is_empty() {
        return true;
    }
    let candidate: PathBuf = dir.join(path_part);
    candidate.exists()
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;

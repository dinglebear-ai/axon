use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".cache", ".next"];

fn is_skipped_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    // Don't skip the root itself (depth 0).
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
    let mut offenders: Vec<PathBuf> = Vec::new();

    let walker = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skipped_dir(e));

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() == "mod.rs" {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_path_buf();
            offenders.push(rel);
        }
    }

    offenders.sort();

    if !offenders.is_empty() {
        eprintln!("ERROR: legacy Rust module roots detected (mod.rs is disallowed):");
        for path in &offenders {
            eprintln!("  {}", path.display());
        }
        eprintln!();
        eprintln!("Use modern module style:");
        eprintln!("  foo.rs + foo/*.rs");
        bail!("found {} mod.rs file(s)", offenders.len());
    }

    println!("OK: no mod.rs files found.");
    Ok(())
}

#[cfg(test)]
#[path = "no_mod_rs_tests.rs"]
mod tests;

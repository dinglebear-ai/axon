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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn find_mod_rs_in_temp_dir() {
        let dir = tempdir().expect("create tempdir");
        let sub = dir.path().join("foo");
        fs::create_dir_all(&sub).expect("mkdir");
        fs::write(sub.join("mod.rs"), "// legacy\n").expect("write mod.rs");

        let result = check(dir.path());
        assert!(result.is_err(), "expected error when mod.rs is present");
    }

    #[test]
    fn passes_when_no_mod_rs() {
        let dir = tempdir().expect("create tempdir");
        let sub = dir.path().join("foo");
        fs::create_dir_all(&sub).expect("mkdir");
        fs::write(sub.join("bar.rs"), "// modern\n").expect("write bar.rs");

        let result = check(dir.path());
        assert!(
            result.is_ok(),
            "expected ok with no mod.rs files: {result:?}"
        );
    }

    #[test]
    fn ignores_target_dir_entries() {
        let dir = tempdir().expect("create tempdir");
        let target = dir.path().join("target").join("foo");
        fs::create_dir_all(&target).expect("mkdir target");
        fs::write(target.join("mod.rs"), "// build artifact\n").expect("write");

        let result = check(dir.path());
        assert!(
            result.is_ok(),
            "expected target/ entries to be skipped: {result:?}"
        );
    }
}

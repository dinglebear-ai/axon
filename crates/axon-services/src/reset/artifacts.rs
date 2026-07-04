//! Artifacts side of `axon reset`: count and (destructively) remove files under
//! the Axon artifact root.

use axon_core::paths::axon_data_base_dir;
use std::path::{Path, PathBuf};

/// Artifact root reset operates on: `$AXON_DATA_DIR/artifacts` (falls back to
/// `~/.axon/artifacts`). This is the same base the MCP artifact writer uses,
/// minus the per-client context subdirectory — reset clears the whole tree.
#[must_use]
pub fn artifact_root() -> PathBuf {
    axon_data_base_dir().join("artifacts")
}

/// Count regular files under `root` (recursive). A missing root is zero files,
/// not an error. Directories and symlinks are not counted as files.
#[must_use]
pub fn count_files(root: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                count += 1;
            }
        }
    }
    count
}

/// Remove every file+subdirectory under `root`, leaving `root` itself as an
/// empty directory. Returns the number of files removed. A missing root removes
/// nothing and is not an error.
pub fn purge_files(root: &Path) -> std::io::Result<usize> {
    let removed = count_files(root);
    if !root.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(root)?.flatten() {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(removed)
}

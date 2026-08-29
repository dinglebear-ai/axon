//! Validation and atomic activation primitives for Codex extension artifacts.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_PATH_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactEntryKind {
    File,
    Directory,
    Symlink,
    Hardlink,
    Device,
    Fifo,
    Socket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub path: PathBuf,
    pub kind: ArtifactEntryKind,
    pub expanded_bytes: u64,
}

pub fn validate_entries(entries: &[ArtifactEntry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err("artifact contains too many entries".to_string());
    }
    let mut total = 0_u64;
    let mut names = HashSet::new();
    for entry in entries {
        if !matches!(
            entry.kind,
            ArtifactEntryKind::File | ArtifactEntryKind::Directory
        ) {
            return Err(format!(
                "unsupported artifact entry type at {}",
                entry.path.display()
            ));
        }
        if entry.path.is_absolute()
            || entry.path.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "artifact path escapes activation root: {}",
                entry.path.display()
            ));
        }
        if entry.path.components().count() > MAX_PATH_DEPTH {
            return Err("artifact path is too deep".to_string());
        }
        let folded = entry.path.to_string_lossy().to_lowercase();
        if !names.insert(folded) {
            return Err("artifact contains a case-folded path collision".to_string());
        }
        total = total
            .checked_add(entry.expanded_bytes)
            .ok_or("artifact expanded size overflow")?;
        if total > MAX_EXPANDED_BYTES {
            return Err("artifact expanded size exceeds limit".to_string());
        }
    }
    Ok(())
}

pub fn activate_directory(staged: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err("artifact destination already exists".to_string());
    }
    let parent = destination
        .parent()
        .ok_or("artifact destination has no parent")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create activation root: {error}"))?;
    fs::rename(staged, destination)
        .map_err(|error| format!("atomic artifact activation failed: {error}"))
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;

//! ACP preflight checks and repairs (symlink fixes, etc.).

use super::config;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SymlinkRepairStats {
    pub scanned_symlinks: usize,
    pub removed_dangling_symlinks: usize,
    pub failed_removals: usize,
}

/// Scan a directory for dangling symlinks and remove them.
pub fn repair_dangling_symlinks(dir: &Path) -> std::io::Result<SymlinkRepairStats> {
    let mut stats = SymlinkRepairStats::default();
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                stats.failed_removals += 1;
                continue;
            }
        };
        let path = entry.path();
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => {
                stats.failed_removals += 1;
                continue;
            }
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        stats.scanned_symlinks += 1;
        match std::fs::metadata(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::remove_file(&path).is_ok() {
                    stats.removed_dangling_symlinks += 1;
                } else {
                    stats.failed_removals += 1;
                }
            }
            Err(_) => {
                stats.failed_removals += 1;
            }
        }
    }
    Ok(stats)
}

/// Repair dangling skill symlinks in the Codex config directory.
pub fn repair_codex_skill_symlinks() -> Result<Option<SymlinkRepairStats>, std::io::Error> {
    let Some(codex_dir) = config::codex_config_dir() else {
        return Ok(None);
    };
    let skills_dir = codex_dir.join("skills");
    if !skills_dir.exists() {
        return Ok(None);
    }
    repair_dangling_symlinks(&skills_dir).map(Some)
}

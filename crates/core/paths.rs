//! Shared path-resolution utilities used across crates.

use std::path::{Path, PathBuf};

/// Returns the trimmed `AXON_DATA_DIR` value if set and non-empty.
pub fn axon_data_dir() -> Option<PathBuf> {
    std::env::var("AXON_DATA_DIR")
        .ok()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// Returns the base data directory for Axon, falling back through:
/// `AXON_DATA_DIR` → `$HOME/.local/share` → `/tmp`.
///
/// Used by subsystems that need a persistent writable directory
/// (prewarm, assistant mode, etc.).
pub fn axon_data_base_dir() -> PathBuf {
    axon_data_dir().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(format!("{home}/.local/share"))
    })
}

/// Extracts the file name from a path string, returning `fallback` if
/// the path has no file name or contains non-UTF-8 components.
pub fn path_basename<'a>(path: &'a str, fallback: &'a str) -> &'a str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_basename_extracts_filename() {
        assert_eq!(path_basename("/usr/bin/claude", "default"), "claude");
        assert_eq!(path_basename("simple", "default"), "simple");
    }

    #[test]
    fn path_basename_uses_fallback_for_empty() {
        assert_eq!(path_basename("", "default"), "default");
    }
}

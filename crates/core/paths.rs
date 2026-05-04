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

/// Returns `~/.axon/` when HOME is set, otherwise `None`.
///
/// Unlike `axon_data_base_dir()`, this returns `None` rather than falling
/// back to `/tmp` — `/tmp` is world-readable/writable and must not be used
/// as a config home (e.g. in systemd units, Docker containers, or CI runners
/// where HOME is unset).
///
/// Callers should skip config loading silently when this returns `None`.
pub fn axon_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".axon"))
}

/// Returns `~/.axon/config.toml` when HOME is set, otherwise `None`.
///
/// When this returns `None` (HOME unset), callers must skip config loading
/// silently — there is no safe fallback path.
pub fn axon_config_path() -> Option<PathBuf> {
    axon_home_dir().map(|d| d.join("config.toml"))
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
    use std::sync::Mutex;

    // Serializes tests that mutate HOME — env mutation is process-wide and not thread-safe.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn path_basename_extracts_filename() {
        assert_eq!(path_basename("/usr/bin/claude", "default"), "claude");
        assert_eq!(path_basename("simple", "default"), "simple");
    }

    #[test]
    fn path_basename_uses_fallback_for_empty() {
        assert_eq!(path_basename("", "default"), "default");
    }

    #[allow(unsafe_code)]
    #[test]
    fn axon_home_dir_returns_some_when_home_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", "/home/testuser") };
        let result = axon_home_dir();
        match saved {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        let path = result.expect("axon_home_dir should return Some when HOME is set");
        assert!(path.to_string_lossy().ends_with(".axon"));
    }

    #[allow(unsafe_code)]
    #[test]
    fn axon_home_dir_returns_none_when_home_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("HOME").ok();
        unsafe { std::env::remove_var("HOME") };
        let result = axon_home_dir();
        match saved {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        assert_eq!(result, None);
    }

    #[allow(unsafe_code)]
    #[test]
    fn axon_config_path_returns_none_when_home_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = std::env::var("HOME").ok();
        unsafe { std::env::remove_var("HOME") };
        let result = axon_config_path();
        match saved {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        assert_eq!(result, None);
    }
}

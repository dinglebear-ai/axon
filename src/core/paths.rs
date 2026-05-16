//! Shared path-resolution utilities used across crates.

use std::path::{Component, Path, PathBuf};

/// Returns the trimmed `AXON_DATA_DIR` value if set and non-empty.
pub fn axon_data_dir() -> Option<PathBuf> {
    std::env::var("AXON_DATA_DIR")
        .ok()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
}

/// Returns the base data directory for Axon, falling back through:
/// `AXON_DATA_DIR` → `$HOME/.axon` → `.cache/axon-rust/data`.
///
/// Used by subsystems that need a persistent writable directory
/// (prewarm, assistant mode, etc.).
pub fn axon_data_base_dir() -> PathBuf {
    axon_data_dir().unwrap_or_else(|| {
        valid_home_path()
            .map(|home| home.join(".axon"))
            .unwrap_or_else(|| PathBuf::from(".cache/axon-rust/data"))
    })
}

/// Returns `~/.axon/` when HOME is set to an absolute path, otherwise `None`.
///
/// Unlike `axon_data_base_dir()`, this returns `None` rather than falling
/// back to `/tmp` — `/tmp` is world-readable/writable and must not be used
/// as a config home (e.g. in systemd units, Docker containers, or CI runners
/// where HOME is unset).
///
/// A non-absolute HOME (e.g. `../somewhere`) or an absolute HOME containing
/// `..` components (e.g. `/tmp/../etc`) can enable path traversal. We reject
/// these values with a warning and return `None`.
///
/// Callers should skip config loading silently when this returns `None`.
pub fn axon_home_dir() -> Option<PathBuf> {
    valid_home_path().map(|home| home.join(".axon"))
}

fn valid_home_path() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())?;

    let home_path = PathBuf::from(&home);
    if !home_path.is_absolute() {
        eprintln!("axon: warning: HOME is not an absolute path ({home:?}); skipping config home");
        return None;
    }
    if home_path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        eprintln!(
            "axon: warning: HOME contains parent-directory components ({home:?}); skipping config home"
        );
        return None;
    }

    Some(home_path)
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

/// Create a directory tree (recursive) with mode 0o700 on Unix.
///
/// Tightens permissions to 0o700 if the directory already exists with a
/// looser mode. Use for any directory under `~/.axon/` that may hold
/// secrets or sensitive runtime state (sqlite jobs.db + WAL/SHM, logs,
/// MCP artifacts, scraped output).
///
/// Falls back to plain `create_dir_all` on non-Unix targets where the
/// 0o700 concept does not apply (Windows uses ACLs).
pub fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        let metadata = std::fs::metadata(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o700 {
            // Tighten before logging so a chmod failure surfaces as Err
            // and the log line never lies about a state that didn't
            // happen. (`?` propagates EPERM/EACCES; the caller decides
            // whether to hard-fail or warn-and-continue.)
            std::fs::set_permissions(path, PermissionsExt::from_mode(0o700))?;
            tracing::warn!(
                path = %path.display(),
                from_mode = format_args!("{mode:o}"),
                "tightened directory permissions to 0700"
            );
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

/// Async wrapper around `ensure_private_dir`: runs the blocking filesystem
/// calls on the tokio blocking pool and folds `JoinError` into `io::Error`.
///
/// Used by callers that need to create a 0o700 directory from an async
/// context (lite-mode SQLite parent dir, MCP artifact root, etc.).
pub async fn ensure_private_dir_async(path: PathBuf) -> std::io::Result<()> {
    tokio::task::spawn_blocking(move || ensure_private_dir(&path))
        .await
        .unwrap_or_else(|e| Err(std::io::Error::other(format!("join error: {e}"))))
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;

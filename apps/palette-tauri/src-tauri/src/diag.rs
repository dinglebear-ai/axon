//! Best-effort diagnostics that survive a packaged build. A desktop bundle
//! launched from a file manager has no attached terminal, so `eprintln!` is
//! discarded; these helpers also append the message to a log file under the
//! Axon data dir (`~/.axon/logs/palette.log`, overridable via
//! `AXON_PALETTE_LOG_PATH`). Append-only; warnings are infrequent so no rotation.

use std::io::Write as _;
use std::path::{Path, PathBuf};

fn log_path() -> Option<PathBuf> {
    std::env::var_os("AXON_PALETTE_LOG_PATH")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".axon/logs/palette.log")))
}

/// Record a warning: always to stderr (dev visibility), best-effort to the log
/// file (recoverable from a packaged build).
pub(crate) fn warn(message: &str) {
    eprintln!("palette: {message}");
    if let Some(path) = log_path() {
        let _ = append_to(&path, message);
    }
}

fn append_to(path: &Path, message: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{ts} WARN palette: {message}")
}

#[cfg(test)]
#[path = "diag_tests.rs"]
mod tests;

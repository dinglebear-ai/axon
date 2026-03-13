//! Session file persistence guard for `pulse_chat`.
//!
//! After a prompt turn completes the Claude Code SDK writes the session `.jsonl`
//! file asynchronously.  `poll_session_file` polls for the file's appearance so
//! the frontend is not signalled to load it before it exists on disk.
//!
//! NOTE: These functions are not yet wired into the production execution path.
//! They exist as infrastructure for an upcoming feature that confirms session
//! file persistence before signalling the frontend.  Individual items are marked
//! `allow(dead_code)` until they gain callers.

use std::path::PathBuf;
use std::time::Duration;

/// Returns the `~/.claude/projects/` directory path, or `None` when the home
/// directory cannot be determined.
///
/// # Platform note
///
/// Axon targets Linux self-hosted deployments exclusively (see project
/// `CLAUDE.md`), so `HOME` is the canonical home-directory variable.  The
/// function also checks `USERPROFILE` as a fallback so the code remains
/// correct if ever compiled for Windows, but `HOME` is always expected to be
/// set in production.
#[allow(dead_code)]
fn projects_dir() -> Option<PathBuf> {
    // Try `HOME` first (Linux / macOS); fall back to `USERPROFILE` (Windows).
    let home = std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|h| !h.is_empty()))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Scan all immediate subdirectories of `projects_dir` for `filename`.
///
/// Claude Code session files are stored as:
///   `~/.claude/projects/<project-slug>/<session-uuid>.jsonl`
///
/// We enumerate project slugs rather than computing one from `cwd` because the
/// slug normalisation algorithm (underscore → dash, etc.) is internal to the
/// Claude Code SDK and can change.
#[allow(dead_code)]
async fn find_session_file(projects_dir: &std::path::Path, filename: &str) -> Option<PathBuf> {
    let mut read_dir = match tokio::fs::read_dir(projects_dir).await {
        Ok(d) => d,
        Err(e) => {
            log::debug!("[session_guard] cannot read projects dir: {e}");
            return None;
        }
    };
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let entry_path = entry.path();
        // Only descend one level — Claude Code project dirs are flat.
        match tokio::fs::metadata(&entry_path).await {
            Ok(m) if m.is_dir() => {}
            _ => continue,
        }
        let candidate = entry_path.join(filename);
        if tokio::fs::metadata(&candidate).await.is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// Poll for `{session_id}.jsonl` under `~/.claude/projects/`.
///
/// Checks every 100 ms for up to 50 attempts (5 seconds total).
/// Returns the absolute path on success, `None` on timeout.
#[allow(dead_code)]
pub(super) async fn poll_session_file(session_id: &str) -> Option<PathBuf> {
    let projects = match projects_dir() {
        Some(p) => p,
        None => {
            log::warn!(
                "[session_guard] home directory env var not set; cannot poll for session file"
            );
            return None;
        }
    };
    let filename = format!("{session_id}.jsonl");

    for attempt in 0..50u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if let Some(path) = find_session_file(&projects, &filename).await {
            log::info!(
                "[session_guard] session file confirmed on disk: {} (attempt {})",
                path.display(),
                attempt
            );
            return Some(path);
        }
        if attempt == 0 {
            log::debug!(
                "[session_guard] waiting for session file: {} in {}",
                filename,
                projects.display()
            );
        }
    }

    log::warn!(
        "[session_guard] session file not found after 5s for session_id={session_id} in {}",
        projects.display()
    );
    None
}

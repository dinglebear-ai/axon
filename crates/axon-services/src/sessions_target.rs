//! Session-selector detection + parsing for `axon source <input>`.
//!
//! Mirrors [`crate::reddit_target`] / [`crate::youtube_target`]: a thin, pure
//! classification + parse wrapper so transports (CLI/MCP/web) can route on
//! session-ness without depending on the legacy `axon-ingest` crate. Unlike
//! reddit/youtube, a session target has **no network acquisition** — the
//! selector already points at an on-disk session export — so this module does
//! the full routing *and* resolution: parse the selector into
//! `(provider, session_id, sessions_root)`.
//!
//! Because a plain directory already routes to the `Local` branch, a session
//! source needs an **explicit prefix** so `axon source ~/.claude/…/foo.jsonl`
//! is not silently indexed as a local code tree. The selector shape is:
//!
//! ```text
//! session:<provider>:<path>
//! ```
//!
//! where `<provider>` is one of `claude` / `codex` / `gemini`, and `<path>` is
//! a session export **file** (claude/codex = `.jsonl`, gemini = a single
//! `.json`) or a directory containing one. The parse derives:
//!
//! * `provider`     = the prefix provider,
//! * `session_id`   = the file stem (the `<path>`'s file name without its
//!   extension) — or, for a directory, the directory's own file name,
//! * `sessions_root`= the file's **parent directory** (or the directory
//!   itself).
//!
//! This is the single-session slice. Indexing a whole directory of sessions
//! (bulk) is a follow-up (see the crate `TODO` note).
//!
//! Session is classified *before* the web branch (it is a prefix, not a URL, so
//! the web catch-all would never match it anyway) alongside the other
//! prefix-based checks.

use std::path::PathBuf;

/// The prefix that marks a session selector.
const SESSION_PREFIX: &str = "session:";

/// Providers whose session exports are understood by the session adapter.
const SESSION_PROVIDERS: &[&str] = &["claude", "codex", "gemini"];

/// A parsed session selector: everything the sessions bridge needs to resolve
/// an on-disk session export to `(sessions_root, provider, session_id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSelector {
    /// Session provider — `claude` / `codex` / `gemini`.
    pub provider: String,
    /// Stable id for this session — the export file stem (or directory name).
    pub session_id: String,
    /// Directory the adapter reads session exports from — the export file's
    /// parent (or the directory itself when the path is a directory).
    pub sessions_root: PathBuf,
}

/// True when `input` should route to the session acquisition path.
///
/// Pure — string parsing only, no I/O — so routing is testable without a data
/// plane. A session selector is any input that parses via
/// [`parse_session_selector`]: a `session:<provider>:<path>` string with a
/// known provider and a non-empty path.
pub fn is_session_selector(input: &str) -> bool {
    parse_session_selector(input).is_ok()
}

/// Parse a `session:<provider>:<path>` selector into a [`SessionSelector`].
///
/// Pure and I/O-free: it does not stat the path, so a nonexistent path still
/// parses (existence is validated later by the sessions bridge when it reads
/// the export). Errors name the bad component without echoing anything
/// sensitive.
pub fn parse_session_selector(input: &str) -> Result<SessionSelector, String> {
    let trimmed = input.trim();
    let rest = trimmed.strip_prefix(SESSION_PREFIX).ok_or_else(|| {
        format!("not a session selector (expected `{SESSION_PREFIX}<provider>:<path>`)")
    })?;

    // Split on the FIRST ':' only — the path itself may contain ':' on some
    // platforms, so we must not split it apart.
    let (provider, path) = rest
        .split_once(':')
        .ok_or_else(|| "session selector is missing a `:<path>` after the provider".to_string())?;

    let provider = provider.trim().to_ascii_lowercase();
    if !SESSION_PROVIDERS.contains(&provider.as_str()) {
        return Err(format!(
            "unknown session provider '{provider}' (expected one of claude/codex/gemini)"
        ));
    }

    let path = path.trim();
    if path.is_empty() {
        return Err("session selector has an empty path".to_string());
    }

    let (sessions_root, session_id) = resolve_root_and_id(path);
    if session_id.is_empty() {
        return Err("session selector path has no resolvable session id".to_string());
    }

    Ok(SessionSelector {
        provider,
        session_id,
        sessions_root,
    })
}

/// Derive `(sessions_root, session_id)` from the selector `<path>` purely from
/// the string shape — no filesystem access.
///
/// `sessions_root` is the path **itself**, not its parent: the sessions adapter
/// special-cases a file root and indexes exactly that one export, while a
/// directory root indexes every export under it (bulk). Pointing at the parent
/// would over-index the whole directory when the user named a single file.
/// `session_id` is the path's file stem (file name without a trailing
/// extension); a path with no file name (e.g. a bare `/`) yields an empty id,
/// which the caller rejects.
fn resolve_root_and_id(path: &str) -> (PathBuf, String) {
    let path_buf = PathBuf::from(path);
    let session_id = path_buf
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToString::to_string)
        .unwrap_or_default();
    (path_buf.clone(), session_id)
}

#[cfg(test)]
#[path = "sessions_target_tests.rs"]
mod tests;

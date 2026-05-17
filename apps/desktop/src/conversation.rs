//! Ask-conversation state for the palette.
//!
//! The palette auto-continues `axon ask` conversations: the first ask starts a
//! conversation, every subsequent ask while the conversation is "live" prepends
//! `--follow-up` to the shell-out. Conversations are in-memory only — they
//! survive across other (non-ask) actions but not across palette restarts.
//!
//! Lifecycle:
//! - Created on the first **successful** `axon ask`
//! - Bumped on each subsequent successful `axon ask`
//! - NOT modified on failed asks (so a transient CLI error doesn't reset the
//!   user's chain)
//! - Cleared by the explicit "Reset ask conversation" action
//! - Cleared by idle timeout (30 minutes since last successful turn)

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Idle window after which a conversation is implicitly reset. Matches the
/// ACP session cache TTL for consistency.
pub(crate) const CONVERSATION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const LATEST_SESSION_FILE: &str = "latest";
const MAX_SESSION_NAME_LEN: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AskConversation {
    /// Number of completed (successful) turns in this conversation. Starts
    /// at 0 before the first ask completes; bumped to 1 on first success.
    pub(crate) turn_count: u32,
    /// Wall-clock instant of the last successful turn. Used for idle-timeout.
    pub(crate) last_turn_at: Instant,
}

impl AskConversation {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            turn_count: 1,
            last_turn_at: now,
        }
    }

    /// Build an `AskConversation` from previously-persisted state recovered
    /// from a CLI session file. Used only by `restore_from_latest`.
    pub(crate) fn from_persisted(turn_count: u32, last_turn_at: Instant) -> Self {
        Self {
            turn_count: turn_count.max(1),
            last_turn_at,
        }
    }

    pub(crate) fn bump(&mut self, now: Instant) {
        self.turn_count = self.turn_count.saturating_add(1);
        self.last_turn_at = now;
    }

    pub(crate) fn is_stale(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_turn_at) >= CONVERSATION_IDLE_TIMEOUT
    }
}

/// Minimal projection of a CLI `AskTurn` JSONL record. We only need the
/// timestamp — `user`/`assistant` text is deliberately NOT deserialized so
/// conversation content never enters palette memory.
#[derive(Debug, Deserialize)]
struct TurnHeader {
    created_at: DateTime<Utc>,
}

/// Try to restore the palette's `AskConversation` state by reading the most
/// recent CLI ask session file under `<home>/.axon/ask-sessions/`.
///
/// Returns `None` (and the palette starts with no live conversation) when:
/// - `<home>/.axon/ask-sessions/latest` does not exist or is empty/malformed
/// - the pointed-to `<name>.jsonl` does not exist (e.g. user ran `axon ask
///   --reset-session` from a terminal)
/// - the file has zero successfully-parsed turns
/// - the last successful turn is older than `CONVERSATION_IDLE_TIMEOUT`
/// - any I/O error occurs while reading
///
/// Returns `Some(AskConversation)` with the recovered turn count and a
/// reconstructed monotonic `last_turn_at` when the latest session is fresh.
///
/// Notes:
/// - The `latest` pointer is sanitized the same way the CLI does; a path-
///   traversal pointer (e.g. `../../etc/passwd`) is rejected as if `latest`
///   did not exist.
/// - Corrupt JSON lines are skipped (matching the CLI's tolerant loader),
///   so a file with valid lines followed by one trailing partial write is
///   recovered up to the last valid turn.
/// - Only the timestamp of each turn is parsed — `user`/`assistant` content
///   never crosses into palette memory.
pub(crate) fn restore_from_latest(home: &Path) -> Option<AskConversation> {
    restore_from_latest_at(home, Utc::now(), Instant::now())
}

/// Test seam: pure function with injected clocks so tests don't depend on
/// `Utc::now()` / `Instant::now()`. Production callers use
/// `restore_from_latest`.
pub(crate) fn restore_from_latest_at(
    home: &Path,
    now_utc: DateTime<Utc>,
    now_instant: Instant,
) -> Option<AskConversation> {
    let dir = sessions_dir(home);

    let session_name = read_latest_session_name(&dir)?;
    let session_path = dir.join(format!("{session_name}.jsonl"));
    let file = match File::open(&session_path) {
        Ok(f) => f,
        Err(err) => {
            tracing::debug!(
                target = "palette::conversation",
                path = %session_path.display(),
                error = %err,
                "ask session file missing or unreadable; starting fresh"
            );
            return None;
        }
    };

    // Stream the file line-by-line rather than loading it all into memory.
    // Each line is parsed into `TurnHeader`, which only captures `created_at` —
    // user/assistant text is dropped as soon as the line buffer is reused, so
    // conversation content never accumulates in palette memory.
    let reader = BufReader::new(file);
    let mut turn_count: u32 = 0;
    let mut last_created_at: Option<DateTime<Utc>> = None;
    for (idx, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(err) => {
                tracing::debug!(
                    target = "palette::conversation",
                    path = %session_path.display(),
                    line = idx + 1,
                    error = %err,
                    "I/O error reading ask-session line; stopping at last good turn"
                );
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TurnHeader>(&line) {
            Ok(header) => {
                turn_count = turn_count.saturating_add(1);
                last_created_at = Some(header.created_at);
            }
            Err(err) => {
                tracing::debug!(
                    target = "palette::conversation",
                    path = %session_path.display(),
                    line = idx + 1,
                    error = %err,
                    "skipping malformed ask-session line"
                );
            }
        }
    }

    // `last_created_at` is `Some` iff at least one turn parsed successfully, so
    // it also gates `turn_count >= 1`. No separate `turn_count == 0` check is
    // needed.
    let last_created_at = last_created_at?;

    // Gate staleness in wall-clock space (robust to system-clock skew between
    // the CLI write and the palette read).
    let elapsed_wall = now_utc
        .signed_duration_since(last_created_at)
        .to_std()
        .unwrap_or(Duration::ZERO);
    if elapsed_wall >= CONVERSATION_IDLE_TIMEOUT {
        tracing::debug!(
            target = "palette::conversation",
            path = %session_path.display(),
            elapsed_secs = elapsed_wall.as_secs(),
            "latest ask session is stale; not restoring"
        );
        return None;
    }

    // Reconstruct the monotonic `Instant` so the in-memory idle-timeout check
    // continues to work the same way for restored and fresh conversations.
    let last_turn_at = now_instant.checked_sub(elapsed_wall).unwrap_or(now_instant);

    Some(AskConversation::from_persisted(turn_count, last_turn_at))
}

fn sessions_dir(home: &Path) -> PathBuf {
    home.join(".axon").join("ask-sessions")
}

fn read_latest_session_name(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join(LATEST_SESSION_FILE)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    sanitize_session_name(trimmed)
}

/// Mirrors `src/cli/commands/ask/followup.rs::sanitize_session_name`. Returns
/// `None` for any name that fails validation rather than substituting a
/// default — a corrupt `latest` pointer should not silently fall back to
/// `default.jsonl`.
fn sanitize_session_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_SESSION_NAME_LEN {
        return None;
    }
    if trimmed == "." || trimmed == ".." {
        return None;
    }
    let safe = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if !safe {
        return None;
    }
    Some(trimmed.to_string())
}

/// Build the argv vector for an `axon` shell-out.
///
/// Extracted as a pure function so the `--follow-up` injection logic can be
/// unit-tested without spawning a subprocess. The conversation parameter is
/// only consulted when `subcommand == "ask"`.
///
/// Note: this mirrors `actions::build_axon_args` but operates on already-split
/// argv pieces and adds the conversation knob. Call this AFTER you've split
/// the user's argument into shell words (or for `ArgMode::Single`, after you
/// have the single-string query).
pub(crate) fn inject_follow_up(
    subcommand: &str,
    args: &mut Vec<String>,
    conversation: Option<&AskConversation>,
) {
    if subcommand != "ask" || conversation.is_none() {
        return;
    }
    // Insert `--follow-up` immediately after the subcommand token in the argv.
    // The argv layout from `build_axon_args` is: ["--local", "<subcommand>",
    // "<arg>...]. We find the subcommand index and insert right after it so
    // the flag binds to `ask` rather than landing in the global-flag block.
    if let Some(idx) = args.iter().position(|a| a == subcommand) {
        args.insert(idx + 1, "--follow-up".to_string());
    }
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;

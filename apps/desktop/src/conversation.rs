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

use std::time::{Duration, Instant};

/// Idle window after which a conversation is implicitly reset. Matches the
/// ACP session cache TTL for consistency.
pub(crate) const CONVERSATION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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

    pub(crate) fn bump(&mut self, now: Instant) {
        self.turn_count = self.turn_count.saturating_add(1);
        self.last_turn_at = now;
    }

    pub(crate) fn is_stale(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_turn_at) >= CONVERSATION_IDLE_TIMEOUT
    }
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

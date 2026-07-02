//! YouTube-target detection for `axon source <input>`.
//!
//! Mirrors [`crate::reddit_target`]: a thin, pure classification wrapper so
//! transports (CLI/MCP/web) can route on youtube-ness without depending on the
//! legacy `axon-ingest` crate. The real parsing/validation lives in
//! [`axon_adapters::youtube::parse_youtube_target`]; this only answers the
//! yes/no routing question.
//!
//! A youtube target is any of:
//! * a video URL — `youtube.com`/`www.youtube.com`/`m.youtube.com`/`youtu.be`
//!   `watch`/`embed`/`shorts`/`v` form,
//! * a playlist or channel URL (`?list=`, `/c/`, `/channel/`, `/user/`, or a
//!   `/@handle`),
//! * a bare `@handle` (expanded to a channel URL), or
//! * a bare 11-character video id (`[A-Za-z0-9_-]{11}`).
//!
//! Youtube is classified *before* the reddit branch: a bare 11-char video id
//! whose characters are all alphanumeric/`_` (no `-`) would also satisfy
//! reddit's bare-subreddit rule, so the *more specific* youtube id check must
//! run first or such an id would be mis-claimed as a subreddit. Youtube is
//! also classified *before* the web branch (youtube.com/youtu.be URLs are
//! http/https, so the web catch-all would otherwise swallow them) and *after*
//! git (a youtube URL carries no git signal, so git never claims it).

use axon_adapters::youtube::parse_youtube_target;

/// True when `input` should route to the youtube acquisition path.
///
/// Pure — string/URL parsing only, no I/O — so routing is testable without a
/// data plane. Delegates to the adapter's `parse_youtube_target`: any input the
/// adapter accepts as a video, playlist, or channel target counts as youtube.
///
/// Note this is intentionally *narrow* for bare words: a bare token only
/// classifies as youtube if it is exactly an 11-character video id
/// (`[A-Za-z0-9_-]{11}`), or a `@handle`. Ordinary words, git URLs, and plain
/// web URLs are rejected by `parse_youtube_target`, so they fall through to
/// their own branches.
pub fn is_youtube_target(input: &str) -> bool {
    parse_youtube_target(input).is_ok()
}

#[cfg(test)]
#[path = "youtube_target_tests.rs"]
mod tests;

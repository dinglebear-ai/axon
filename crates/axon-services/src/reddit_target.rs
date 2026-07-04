//! Reddit-target detection for `axon source <input>`.
//!
//! Mirrors [`crate::feed_target`]: a thin, pure classification wrapper so
//! transports (CLI/MCP/web) can route on reddit-ness without depending on the
//! legacy `axon-ingest` crate. The real parsing/validation lives in
//! [`axon_adapters::reddit::parse_reddit_target`]; this only answers the yes/no
//! routing question.
//!
//! A reddit target is either:
//! * a subreddit reference — `r/<name>` or `/r/<name>` (or a bare valid
//!   subreddit name), or
//! * a reddit.com thread URL (`reddit.com`/`www.reddit.com`/`old.reddit.com`).
//!
//! Reddit is classified *before* the web branch (reddit.com URLs are http/https,
//! so the web catch-all would otherwise swallow them) and *after* git (a
//! reddit.com URL carries no git signal, so git never claims it).

use axon_adapters::reddit::parse_reddit_target;

/// True when `input` should route to the reddit acquisition path.
///
/// Pure — string parsing only, no I/O — so routing is testable without a data
/// plane. Delegates to the adapter's `parse_reddit_target`: any input the
/// adapter accepts as a subreddit or thread target counts as reddit.
///
/// Note this is intentionally *narrow*: a bare word only classifies as reddit if
/// it is a syntactically valid subreddit name (3–21 chars, alphanumeric/`_`).
/// Ordinary web URLs (non-reddit hosts) and git URLs are rejected by
/// `parse_reddit_target`, so they fall through to their own branches.
pub fn is_reddit_target(input: &str) -> bool {
    parse_reddit_target(input).is_ok()
}

#[cfg(test)]
#[path = "reddit_target_tests.rs"]
mod tests;

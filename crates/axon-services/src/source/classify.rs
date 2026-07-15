//! Acquisition-class routing for `index_source`.
//!
//! Relocated verbatim (behavior-preserving) from the CLI's
//! `commands/source.rs::classify_source_input`. The exact ordering is
//! load-bearing and covered by the source classification tests:
//!
//! 1. **Local existence wins first** — a directory literally named like a URL is
//!    still treated as local.
//! 2. Explicit `session:`/`pkg:` prefix selectors — not paths or URLs, so no
//!    other class can claim them.
//! 3. A genuine git target (known host or explicit git marker).
//! 4. A feed URL (RSS/Atom/RDF or `rss:`/`feed:`/`atom:` prefix).
//! 5. A youtube target (checked *before* reddit: a bare 11-char alphanumeric
//!    video id also satisfies reddit's bare-subreddit rule, so the more specific
//!    youtube check must run first).
//! 6. A reddit target (`r/<name>` subreddit or reddit.com thread URL).
//! 7. A plain http/https web URL (catch-all).
//! 8. Unsupported.
//!
//! Feed, youtube, AND reddit classification MUST precede the web branch: feed
//! URLs, youtube.com/youtu.be URLs, and reddit.com thread URLs are all
//! http/https, so the web catch-all would otherwise swallow them.

use std::path::PathBuf;

use axon_api::source::SafetyClass;

/// Acquisition class the input routes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceInputKind {
    /// An existing path on the local filesystem.
    Local,
    /// A parseable git repository URL (github/gitlab/gitea/`.git`/`git+https`).
    Git,
    /// An RSS/Atom/RDF feed URL (or `rss:`/`feed:`/`atom:` prefix).
    Feed,
    /// A youtube video/playlist/channel URL, `@handle`, or bare 11-char video id.
    Youtube,
    /// A reddit subreddit (`r/<name>`) or reddit.com thread URL.
    Reddit,
    /// An http/https URL that is not a git, feed, youtube, or reddit target.
    Web,
    /// A `session:<provider>:<path>` selector (provider ∈ {claude,codex,gemini}).
    Session,
    /// A `pkg:<registry>/<package>` target (registry ∈ {npm,pypi,crates}).
    Registry,
    /// None of the above — unsupported.
    Unsupported,
}

/// Classify `input` into an acquisition class.
///
/// Split out as a pure-ish async fn (only fs metadata + string parsing) so
/// routing is testable without a data plane.
pub async fn classify_source_input(input: &str) -> SourceInputKind {
    if input_is_local_path(input).await {
        return SourceInputKind::Local;
    }
    // Explicit `session:`/`pkg:` prefix selectors are checked before the URL
    // branches — they are not paths or URLs, so no other class can claim them.
    if crate::is_session_selector(input) {
        return SourceInputKind::Session;
    }
    if crate::is_registry_target(input) {
        return SourceInputKind::Registry;
    }
    if input_is_git_target(input) {
        return SourceInputKind::Git;
    }
    if crate::is_feed_target(input) {
        return SourceInputKind::Feed;
    }
    if crate::is_youtube_target(input) {
        return SourceInputKind::Youtube;
    }
    if crate::is_reddit_target(input) {
        return SourceInputKind::Reddit;
    }
    if input_is_web_url(input) {
        return SourceInputKind::Web;
    }
    SourceInputKind::Unsupported
}

/// Map a classified source input to its [`SafetyClass`].
///
/// This is the single classifier shared by every transport (REST's
/// `crates/axon-web/src/server/handlers/sources.rs` and MCP's
/// `crates/axon-mcp/src/server/handlers_source.rs`): both authorize a source
/// request by classifying it here, then resolving the fine-grained scope that
/// class requires via `axon_authz::required_scope_for_safety_class`. Keeping
/// the mapping in one place means a local-filesystem source is upgraded to
/// `axon:local` identically on every transport — a transport that duplicated
/// (or forgot to call) this mapping could let a caller holding only the
/// broad `axon:write` scope index an arbitrary local path.
///
/// `Local` and `Session` map to `SafetyClass::LocalFilesystem` because both
/// read server-local paths. Most other classified kinds acquire over the network
/// and fall back to `PublicNetwork`. CLI/MCP tool-execution sources are not
/// produced by [`classify_source_input`] today; when they are, they should map
/// to `SafetyClass::ToolExecution`.
pub fn safety_class_for(kind: SourceInputKind) -> SafetyClass {
    match kind {
        SourceInputKind::Local | SourceInputKind::Session => SafetyClass::LocalFilesystem,
        _ => SafetyClass::PublicNetwork,
    }
}

/// True when `input` parses as an http/https URL.
fn input_is_web_url(input: &str) -> bool {
    match url::Url::parse(input) {
        Ok(parsed) => matches!(parsed.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

/// True when `input` resolves to an existing path on disk or uses the same
/// lexical local-path prefixes the source router uses for local identity.
async fn input_is_local_path(input: &str) -> bool {
    axon_route::canonical::is_lexically_local_path(input)
        || tokio::fs::metadata(PathBuf::from(input)).await.is_ok()
}

/// True when `input` should route to the git clone path.
///
/// [`crate::is_git_target`] alone is too permissive for routing: it accepts
/// *any* `https://host/path` as a cloneable repo (unknown hosts get the generic
/// `git` provider), which would swallow ordinary web URLs like
/// `https://docs.example.com/guide`. For routing we require a genuine git signal
/// on top of it — a known git host or an explicit git marker (`.git` suffix,
/// `git+`/`git:` prefix) — so plain web URLs fall through to the web branch. The
/// git clone path itself still uses the permissive parser.
fn input_is_git_target(input: &str) -> bool {
    crate::is_git_target(input) && has_git_signal(input)
}

/// Whether `input` carries an explicit git signal (known host or git marker).
fn has_git_signal(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.starts_with("git+") || trimmed.starts_with("git:") {
        return true;
    }
    if let Ok(parsed) = url::Url::parse(trimmed.strip_prefix("git+").unwrap_or(trimmed)) {
        if parsed.path().trim_end_matches('/').ends_with(".git") {
            return true;
        }
        if let Some(host) = parsed.host_str() {
            let host = host.to_ascii_lowercase();
            return host.contains("github")
                || host.contains("gitlab")
                || host.contains("gitea")
                || host.contains("forgejo")
                || host.contains("codeberg");
        }
    }
    false
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;

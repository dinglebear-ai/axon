//! `axon source <input>` — index a source through the unified pipeline.
//!
//! This is the first user-facing surface of the new source pipeline. It handles
//! three acquisition classes:
//!
//! * **Local paths** — dispatched to the target local-source runtime via
//!   [`axon_services::index_local_source_with_job`].
//! * **Git repository URLs** — shallow-cloned (acquisition) then dispatched to
//!   the git bridge via [`axon_services::index_git_source_with_job`].
//! * **Feed URLs** (RSS/Atom/RDF, or an explicit `rss:`/`feed:`/`atom:` prefix)
//!   — fetched to a prepared document then dispatched to the feed bridge via
//!   [`axon_services::index_feed_source_with_job`]. Classified *before* the web
//!   branch so a feed URL (which is also http/https) is not swallowed by the
//!   web catch-all.
//! * **Reddit targets** (`r/<name>` subreddits or reddit.com thread URLs) —
//!   OAuth-fetched to a prepared JSON dump then dispatched to the reddit bridge
//!   via [`axon_services::index_reddit_source_with_job`]. Classified *before*
//!   the web branch so a reddit.com thread URL (also http/https) is not
//!   swallowed by the web catch-all.
//! * **Web URLs** (http/https, not a git, feed, or reddit target) — crawled to
//!   completion then dispatched to the web bridge via
//!   [`axon_services::index_web_source_with_job`]. This is the canonical
//!   replacement for `axon crawl <url>`.
//!
//! Everything else (youtube/sessions/registry acquisition) returns a clear "not
//! yet wired" error — a later P10 slice.

mod feed;
mod git;
mod reddit;
mod web;
mod youtube;

use axon_api::source::JobId;
use axon_core::config::Config;
use axon_core::logging::log_info;
use axon_core::ui::{accent, muted, primary};
use axon_services::context::{ServiceContext, TargetLocalSourceRuntime};
use axon_services::{
    LocalSourceIndexInput, LocalSourceIndexOutput, LocalSourceSelectionPolicy,
    index_local_source_with_job,
};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

/// Stable owner id used to lease sources indexed from the CLI.
pub(crate) const CLI_OWNER_ID: &str = "cli";

/// Acquisition class the input routes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceInputKind {
    /// An existing path on the local filesystem.
    Local,
    /// A parseable git repository URL (github/gitlab/gitea/`.git`/`git+https`).
    Git,
    /// An RSS/Atom/RDF feed URL (or `rss:`/`feed:`/`atom:` prefix) — fetched +
    /// indexed through the feed bridge.
    Feed,
    /// A youtube video/playlist/channel URL, `@handle`, or bare 11-char video
    /// id — yt-dlp-fetched to a prepared dump + indexed through the youtube
    /// bridge.
    Youtube,
    /// A reddit subreddit (`r/<name>`) or reddit.com thread URL — OAuth-fetched
    /// to a prepared dump + indexed through the reddit bridge.
    Reddit,
    /// An http/https URL that is not a git, feed, youtube, or reddit target —
    /// crawled + indexed.
    Web,
    /// None of the above — unsupported for this slice.
    Unsupported,
}

pub async fn run_source(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let input = resolve_source_input(cfg)?;

    match classify_source_input(&input).await {
        SourceInputKind::Local => run_local_source(cfg, service_context, &input).await,
        SourceInputKind::Git => {
            let runtime = require_data_plane(service_context)?;
            git::run_git_source(cfg, runtime, &input).await
        }
        SourceInputKind::Feed => {
            let runtime = require_data_plane(service_context)?;
            feed::run_feed_source(cfg, runtime, &input).await
        }
        SourceInputKind::Youtube => {
            let runtime = require_data_plane(service_context)?;
            youtube::run_youtube_source(cfg, runtime, &input).await
        }
        SourceInputKind::Reddit => {
            let runtime = require_data_plane(service_context)?;
            reddit::run_reddit_source(cfg, runtime, &input).await
        }
        SourceInputKind::Web => {
            let runtime = require_data_plane(service_context)?;
            web::run_web_source(cfg, runtime, &input).await
        }
        SourceInputKind::Unsupported => Err(unsupported_input_error(&input)),
    }
}

/// Classify the input into an acquisition class.
///
/// Local existence wins first (a directory literally named like a URL is still
/// treated as local), then a genuine git target, then a feed URL, then a
/// youtube target, then a reddit target, then a plain http/https web URL, then
/// unsupported. Feed, youtube, AND reddit classification MUST precede the web
/// branch: feed URLs, youtube.com/youtu.be URLs, and reddit.com thread URLs are
/// all http/https, so the web catch-all would otherwise swallow them.
///
/// Youtube is checked *before reddit*: a bare 11-char video id whose characters
/// are all alphanumeric/`_` (no `-`) would also satisfy reddit's bare-subreddit
/// rule, so the more specific youtube id check must run first or such an id
/// would be mis-claimed as a subreddit. Both are checked *after* git (a
/// youtube/reddit URL carries no git signal, so git never claims it). Split out
/// as a pure-ish async fn (only fs metadata + string parsing) so routing is
/// testable without a data plane.
async fn classify_source_input(input: &str) -> SourceInputKind {
    if input_is_local_path(input).await {
        return SourceInputKind::Local;
    }
    if input_is_git_target(input) {
        return SourceInputKind::Git;
    }
    if axon_services::is_feed_target(input) {
        return SourceInputKind::Feed;
    }
    if axon_services::is_youtube_target(input) {
        return SourceInputKind::Youtube;
    }
    if axon_services::is_reddit_target(input) {
        return SourceInputKind::Reddit;
    }
    if input_is_web_url(input) {
        return SourceInputKind::Web;
    }
    SourceInputKind::Unsupported
}

/// True when `input` parses as an http/https URL. Checked only after git-target
/// classification, so plain web URLs (docs sites, blogs) route here while
/// git-hosting URLs still route to the git clone path.
fn input_is_web_url(input: &str) -> bool {
    match url::Url::parse(input) {
        Ok(parsed) => matches!(parsed.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

/// True when `input` resolves to an existing path on disk.
async fn input_is_local_path(input: &str) -> bool {
    tokio::fs::metadata(PathBuf::from(input)).await.is_ok()
}

/// True when `input` should route to the git clone path.
///
/// [`axon_services::is_git_target`] alone is too permissive for routing: it
/// accepts *any* `https://host/path` as a cloneable repo (unknown hosts get the
/// generic `git` provider), which would swallow ordinary web URLs like
/// `https://docs.example.com/guide`. For `axon source` routing we require a
/// genuine git signal on top of it — a known git host or an explicit git marker
/// (`.git` suffix, `git+`/`git:` prefix) — so plain web URLs fall through to the
/// web branch. The git clone path itself still uses the permissive parser.
fn input_is_git_target(input: &str) -> bool {
    axon_services::is_git_target(input) && has_git_signal(input)
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

/// Read the positional argument, mirroring how `run_embed` resolves input.
fn resolve_source_input(cfg: &Config) -> Result<String, Box<dyn Error>> {
    cfg.positional
        .first()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "axon source requires a local path, git repository URL, feed URL, youtube target, \
             reddit target, or web URL argument"
                .into()
        })
}

/// Require the target source runtime (the data plane), or return the shared
/// "data plane required" guard error.
fn require_data_plane(
    service_context: &ServiceContext,
) -> Result<&TargetLocalSourceRuntime, Box<dyn Error>> {
    service_context
        .target_local_source_runtime()
        .ok_or_else(|| -> Box<dyn Error> {
            "source indexing requires a running data plane (set qdrant_url + tei_url; \
             available under serve/mcp/--wait)"
                .into()
        })
}

/// Clear error for inputs that are not a local path, git URL, feed URL, youtube
/// target, reddit target, or web URL.
fn unsupported_input_error(input: &str) -> Box<dyn Error> {
    format!(
        "axon source supports local paths, git repository URLs, feed URLs, youtube targets \
         (a video/playlist/channel URL, @handle, or 11-char video id), reddit targets \
         (r/<name> or a reddit.com thread URL), and web URLs; {input} is none of these \
         (sessions/registry acquisition is a P10 follow-up)"
    )
    .into()
}

async fn run_local_source(
    cfg: &Config,
    service_context: &ServiceContext,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    let runtime = require_data_plane(service_context)?;
    let root = PathBuf::from(input);
    log_info(&format!(
        "command=source collection={} kind=local",
        cfg.collection
    ));

    let index_input = build_index_input(cfg, runtime, root);
    let output = index_local_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await?;

    render_source_output(cfg, input, &output);
    Ok(())
}

fn build_index_input(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    root: PathBuf,
) -> LocalSourceIndexInput {
    LocalSourceIndexInput {
        root,
        collection: cfg.collection.clone(),
        owner_id: CLI_OWNER_ID.to_string(),
        // Placeholder — `index_local_source_with_job` creates the real job and
        // overwrites this with the descriptor's job id.
        job_id: JobId::new(Uuid::nil()),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
    }
}

fn render_source_output(cfg: &Config, input: &str, output: &LocalSourceIndexOutput) {
    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "job_id": output.job_id.0.to_string(),
                "source_id": output.source_id.0,
                "generation": output.generation.0,
                "documents_prepared": output.documents_prepared,
                "chunks_prepared": output.chunks_prepared,
                "vector_points_written": output.vector_points_written,
                "removed_files": output.removed_files,
                "target": input,
                "collection": cfg.collection,
            })
        );
        return;
    }

    println!(
        "  {} {}",
        primary("Source Indexed"),
        accent(&output.source_id.0)
    );
    println!("  {}", muted(&format!("Input: {input}")));
    println!(
        "  {}",
        muted(&format!("Generation: {}", output.generation.0))
    );
    println!(
        "  {}",
        muted(&format!(
            "Documents: {}  Chunks: {}  Vector points: {}  Removed: {}",
            output.documents_prepared,
            output.chunks_prepared,
            output.vector_points_written,
            output.removed_files,
        ))
    );
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;

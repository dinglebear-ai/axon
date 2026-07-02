//! Prepared Reddit JSON dump parsing.
//!
//! Like the `git` adapter reads an already-cloned repo root, this adapter
//! reads an already-fetched Reddit JSON dump (a `reddit_dump_path` option
//! pointing at a file the caller — the services bridge — produced via the
//! Reddit OAuth API). This keeps the adapter network-free and unit-testable
//! with fixture dumps.
//!
//! The dump is a JSON array of post objects, one per subreddit post or
//! standalone thread. Each entry carries the Reddit API's `post["data"]`
//! shape plus an optional pre-flattened `comments` array (each comment
//! already recursively resolved and score/depth-filtered by the bridge,
//! matching the legacy `CommentWithContext` shape). This mirrors what the
//! legacy `axon-ingest::reddit` module built from the live API before
//! embedding, minus the network fetch itself.

use axon_api::source::ApiError;
use axon_error::ErrorStage;
use serde::Deserialize;

use crate::adapter::Result;

/// One post entry in a prepared Reddit dump file.
#[derive(Debug, Clone, Deserialize)]
pub struct RedditDumpItem {
    pub title: Option<String>,
    #[serde(default)]
    pub selftext: Option<String>,
    pub permalink: Option<String>,
    pub author: Option<String>,
    pub score: Option<i64>,
    pub subreddit: Option<String>,
    pub domain: Option<String>,
    pub num_comments: Option<u64>,
    pub upvote_ratio: Option<f64>,
    pub is_video: Option<bool>,
    pub distinguished: Option<String>,
    pub gilded: Option<u64>,
    pub link_flair_text: Option<String>,
    pub created_utc: Option<u64>,
    #[serde(default)]
    pub comments: Vec<RedditDumpComment>,
}

impl RedditDumpItem {
    pub(super) fn author_or_deleted(&self) -> String {
        self.author
            .clone()
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "[deleted]".to_string())
    }

    /// Reddit `kind` label: `t3` for a post/link, matching the API's own
    /// "thing" prefix convention. Dumps are post-level only; comments are
    /// folded into content, not emitted as separate manifest items.
    pub(super) fn kind_label(&self) -> &'static str {
        "t3"
    }

    /// Build the post's canonical `https://www.reddit.com<permalink>` URL.
    pub(super) fn canonical_url(&self) -> String {
        let permalink = self.permalink.clone().unwrap_or_default();
        format!("https://www.reddit.com{permalink}")
    }

    /// Render post + comments into a single markdown-ish text body, matching
    /// the legacy `# {title}\n\n{selftext}` + per-comment `---` blocks.
    pub(super) fn render_content(&self) -> String {
        let title = self.title.as_deref().unwrap_or("Untitled");
        let mut content = format!("# {title}");
        if let Some(selftext) = self.selftext.as_deref().filter(|s| !s.is_empty()) {
            content.push_str(&format!("\n\n{selftext}"));
        }
        for comment in &self.comments {
            let mut ctx = format!("\n\n---\nPost: {title}\n\n");
            if let Some(parent) = comment.parent_text.as_deref().filter(|p| !p.is_empty()) {
                ctx.push_str(&format!("Replying to: {parent}\n\n"));
            }
            ctx.push_str(&comment.body);
            content.push_str(&ctx);
        }
        content
    }
}

/// A single flattened comment in a prepared dump, matching the legacy
/// `CommentWithContext` shape (body + optional parent text for threading).
#[derive(Debug, Clone, Deserialize)]
pub struct RedditDumpComment {
    pub body: String,
    #[serde(default)]
    pub parent_text: Option<String>,
}

/// Parse a prepared Reddit JSON dump file into its post items.
///
/// Public so the services acquire mapping (`axon_services::reddit_acquire`)
/// can round-trip its produced dump JSON through the adapter's real
/// deserialize path in tests — mirroring how `axon_adapters::youtube::dump`
/// exposes `read_youtube_dump`. A field/serde drift in these dump structs
/// then breaks that round-trip test.
pub fn parse_dump(bytes: &[u8]) -> Result<Vec<RedditDumpItem>> {
    serde_json::from_slice::<Vec<RedditDumpItem>>(bytes).map_err(|err| {
        ApiError::new(
            "adapter.reddit.dump_invalid",
            ErrorStage::Discovering,
            format!("reddit dump is not a valid JSON array of post items: {err}"),
        )
    })
}

#[cfg(test)]
#[path = "dump_tests.rs"]
mod tests;

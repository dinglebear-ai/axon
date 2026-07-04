//! Feed target resolution — the prepared feed document path, and a small
//! dependency-light HTML→text conversion for entry bodies. Full markdown
//! fidelity (headings, links, lists) is handled by `axon-core::content` in
//! the legacy ingest path; this adapter only needs plain, embeddable text and
//! stays out of the heavy `axon-core` dependency graph.

use std::path::PathBuf;

use axon_api::source::{ApiError, SourcePlan};
use serde_json::Value;

use crate::adapter::Result;

/// The prepared feed document path, passed by the services bridge as a
/// validated option (mirrors the `git` adapter's `repo_root`).
pub fn feed_path(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("feed_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.feed.feed_path.required",
                axon_error::ErrorStage::Planning,
                "feed adapter requires a feed_path option pointing at a prepared feed document",
            )
        })
}

/// Strip HTML tags down to plain text. Not a full markdown converter — feed
/// entry bodies are frequently truncated summaries, so a lossless conversion
/// isn't worth the dependency weight. Collapses whitespace and decodes the
/// handful of HTML entities feeds commonly emit.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let decoded = decode_entities(&out);
    collapse_whitespace(&decoded)
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;

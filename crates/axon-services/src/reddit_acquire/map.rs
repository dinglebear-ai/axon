//! Pure raw-Reddit-JSON → prepared-dump mapping.
//!
//! No network, no I/O — just structural mapping from the Reddit API's JSON
//! (`Listing`/`t3`/`t1` "things") into the dump shape the
//! `axon_adapters::reddit` adapter reads. The serialized field names here MUST
//! match [`axon_adapters::reddit::dump::RedditDumpItem`] /
//! [`axon_adapters::reddit::dump::RedditDumpComment`] exactly, because the
//! adapter deserializes the file this produces. This is the highest-risk part of
//! the reddit acquisition slice, hence it is isolated and unit-tested with
//! fixtures.

use serde::Serialize;
use serde_json::Value;

/// One post entry in a prepared Reddit dump. Field names mirror
/// `axon_adapters::reddit::dump::RedditDumpItem` (the adapter's deserialize
/// target). `comments` is flattened + score-filtered by [`flatten_comments`].
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct DumpItem {
    pub title: Option<String>,
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
    pub comments: Vec<DumpComment>,
}

/// One flattened comment in a prepared dump. Field names mirror
/// `axon_adapters::reddit::dump::RedditDumpComment`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct DumpComment {
    pub body: String,
    pub parent_text: Option<String>,
}

/// Comments below this score are dropped, matching the legacy default filter
/// (`reddit_min_score` default). Deleted/removed bodies are always dropped.
const MIN_COMMENT_SCORE: i64 = 1;

/// Maximum comment-tree recursion depth. Bounds work on adversarially deep
/// threads independent of the API's own `depth` request.
const MAX_COMMENT_DEPTH: usize = 10;

/// Map a subreddit hot-listing response (`{"data":{"children":[{t3}...]}}`) into
/// dump items. Posts with no post data are skipped. Listing responses carry no
/// comment tree, so each item's `comments` is empty (the adapter renders the
/// post body alone).
pub(super) fn map_subreddit_listing(value: &Value) -> Vec<DumpItem> {
    let Some(children) = value["data"]["children"].as_array() else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|child| child.get("data"))
        .map(|data| map_post_data(data, Vec::new()))
        .collect()
}

/// Map a thread `.json` response (a 2-element array: `[postListing,
/// commentListing]`) into a single dump item with its comment tree flattened.
/// Returns `None` when the post `t3` data is absent.
pub(super) fn map_thread(value: &Value) -> Option<DumpItem> {
    let post_data = &value[0]["data"]["children"][0]["data"];
    if !post_data.is_object() {
        return None;
    }
    let mut comments = Vec::new();
    if let Some(comment_root) = value[1].get("data") {
        flatten_comments(comment_root, 1, None, &mut comments);
    }
    Some(map_post_data(post_data, comments))
}

/// Build a [`DumpItem`] from a post's `data` object plus already-flattened
/// comments. Missing/typed-wrong fields degrade to `None` rather than failing —
/// the adapter tolerates absent fields.
fn map_post_data(data: &Value, comments: Vec<DumpComment>) -> DumpItem {
    DumpItem {
        title: str_field(data, "title"),
        selftext: str_field(data, "selftext"),
        permalink: str_field(data, "permalink"),
        author: str_field(data, "author"),
        score: data["score"].as_i64(),
        subreddit: str_field(data, "subreddit"),
        domain: str_field(data, "domain"),
        num_comments: data["num_comments"].as_u64(),
        upvote_ratio: data["upvote_ratio"].as_f64(),
        is_video: data["is_video"].as_bool(),
        distinguished: str_field(data, "distinguished"),
        gilded: data["gilded"].as_u64(),
        link_flair_text: str_field(data, "link_flair_text"),
        created_utc: created_utc(data),
        comments,
    }
}

/// Recursively flatten a Reddit comment listing into `out`, dropping non-`t1`
/// things, deleted/removed/empty bodies, and comments below [`MIN_COMMENT_SCORE`].
/// `parent_text` carries the immediate parent comment body for threading
/// context (matching the legacy `CommentWithContext` shape).
fn flatten_comments(
    listing: &Value,
    depth: usize,
    parent_text: Option<&str>,
    out: &mut Vec<DumpComment>,
) {
    if depth > MAX_COMMENT_DEPTH {
        return;
    }
    let Some(children) = listing["children"].as_array() else {
        return;
    };
    for child in children {
        if child["kind"].as_str() != Some("t1") {
            continue;
        }
        let data = &child["data"];
        let score = data["score"].as_i64().unwrap_or(0);
        if score < MIN_COMMENT_SCORE {
            continue;
        }
        let body = data["body"].as_str().unwrap_or("");
        if body.is_empty() || body == "[deleted]" || body == "[removed]" {
            continue;
        }
        out.push(DumpComment {
            body: body.to_string(),
            parent_text: parent_text.map(str::to_string),
        });
        let replies = &data["replies"];
        if replies.is_object() && replies["data"].is_object() {
            flatten_comments(&replies["data"], depth + 1, Some(body), out);
        }
    }
}

/// Read a string field, treating empty strings as absent for the optional
/// text fields Reddit returns as `""` when unset.
fn str_field(data: &Value, key: &str) -> Option<String> {
    data[key]
        .as_str()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

/// Reddit sends `created_utc` as a float epoch-seconds value (e.g. `1.7e9`).
/// Truncate to whole seconds for the dump's `u64` field; negative values are
/// dropped.
fn created_utc(data: &Value) -> Option<u64> {
    let raw = data["created_utc"].as_f64()?;
    if raw < 0.0 {
        return None;
    }
    Some(raw as u64)
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;

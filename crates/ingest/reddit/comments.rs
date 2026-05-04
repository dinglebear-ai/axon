use super::client::fetch_reddit_json_with_cancel;
use super::types::CommentWithContext;
use crate::crates::core::config::Config;
use std::error::Error;
use tokio_util::sync::CancellationToken;

/// Recursively traverse a Reddit comment tree up to max_depth, filtering by min_score.
/// Borrows from `data` -- no cloning of JSON subtrees. Only the body text string is
/// extracted and owned by `CommentWithContext`.
pub(super) fn collect_comments_recursive(
    data: &serde_json::Value,
    current_depth: usize,
    max_depth: usize,
    min_score: i32,
    parent_text: Option<&str>,
    out: &mut Vec<CommentWithContext>,
) {
    if current_depth > max_depth {
        return;
    }

    let Some(children) = data["children"].as_array() else {
        return;
    };
    for child in children {
        let kind = child["kind"].as_str().unwrap_or("");
        if kind != "t1" {
            continue;
        }
        let c_data = &child["data"];
        let score = c_data["score"].as_i64().unwrap_or(0) as i32;
        if score < min_score {
            continue;
        }

        let body = c_data["body"].as_str().unwrap_or("");
        if body.is_empty() || body == "[deleted]" || body == "[removed]" {
            continue;
        }

        out.push(CommentWithContext {
            body: body.to_string(),
            parent_text: parent_text.map(str::to_string),
        });

        let replies = &c_data["replies"];
        if replies.is_object() && replies["data"].is_object() {
            collect_comments_recursive(
                &replies["data"],
                current_depth + 1,
                max_depth,
                min_score,
                Some(body),
                out,
            );
        }
    }
}

/// Fetch comments for a thread permalink, recursively up to cfg.reddit_depth.
pub(super) async fn fetch_thread_comments(
    cfg: &Config,
    token: &str,
    permalink: &str,
    cancel_token: Option<&CancellationToken>,
) -> Result<Vec<CommentWithContext>, Box<dyn Error>> {
    let clean = permalink.trim_end_matches('/');
    let json_url = format!(
        "https://oauth.reddit.com{clean}.json?limit=100&depth={}",
        cfg.reddit_depth.max(1)
    );

    let resp = fetch_reddit_json_with_cancel(&json_url, token, cancel_token).await?;

    let mut comments = Vec::new();
    if let Some(data) = resp[1].get("data") {
        collect_comments_recursive(
            data,
            1,
            cfg.reddit_depth,
            cfg.reddit_min_score,
            None,
            &mut comments,
        );
    }

    Ok(comments)
}

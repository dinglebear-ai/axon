mod client;
mod comments;
mod meta;
mod types;

pub use client::get_access_token;
pub use types::{RedditTarget, classify_target};

use crate::crates::core::config::{Config, RedditSort};
use crate::crates::core::content::url_to_domain;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;

const PHASE_AUTHENTICATING: &str = "authenticating";
const PHASE_FETCHING_POSTS: &str = "fetching_posts";
const PHASE_EMBEDDING_POSTS: &str = "embedding_posts";

use client::fetch_reddit_json;
use comments::{collect_comments_recursive, fetch_thread_comments};
use types::{CommentWithContext, validate_subreddit};

/// Batch flush threshold — accumulate docs then embed in one call.
const REDDIT_BATCH_FLUSH_SIZE: usize = 50;

/// Embed posts from a subreddit concurrently, including recursive comments per post.
///
/// Posts are fetched and prepared concurrently, then accumulated into a buffer
/// and flushed in batches of REDDIT_BATCH_FLUSH_SIZE to `embed_prepared_docs`.
/// This avoids the N+1 anti-pattern of one TEI + Qdrant round-trip per post.
async fn ingest_subreddit(
    cfg: &Config,
    token: &str,
    name: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    reporter.report_phase(PHASE_FETCHING_POSTS).await;
    validate_subreddit(name)?;

    use futures_util::StreamExt;

    let mut after = String::new();
    let mut fetched_posts = 0usize;
    let max_posts = cfg.reddit_max_posts;

    // Channel for docs produced by concurrent post processors → batch drain.
    let (doc_tx, mut doc_rx) = mpsc::channel::<PreparedDoc>(256);

    // Spawn a drain task that flushes docs in batches.
    let drain_cfg = cfg.clone();
    let drain_handle = tokio::spawn(async move {
        let mut total_chunks = 0usize;
        let mut buffer: Vec<PreparedDoc> = Vec::with_capacity(REDDIT_BATCH_FLUSH_SIZE);

        while let Some(doc) = doc_rx.recv().await {
            buffer.push(doc);
            if buffer.len() >= REDDIT_BATCH_FLUSH_SIZE {
                total_chunks += flush_batch(&drain_cfg, &mut buffer).await;
            }
        }
        // Flush remaining docs after channel closes.
        if !buffer.is_empty() {
            total_chunks += flush_batch(&drain_cfg, &mut buffer).await;
        }
        total_chunks
    });

    loop {
        let limit = if max_posts > 0 {
            (max_posts - fetched_posts).min(100)
        } else {
            100
        };
        let mut url = format!(
            "https://oauth.reddit.com/r/{name}/{}?limit={limit}&raw_json=1",
            cfg.reddit_sort,
        );
        if cfg.reddit_sort == RedditSort::Top {
            url.push_str(&format!("&t={}", cfg.reddit_time));
        }
        if !after.is_empty() {
            url.push_str(&format!("&after={after}"));
        }

        let resp = fetch_reddit_json(&url, token).await?;
        if let Some(msg) = resp["message"].as_str() {
            return Err(format!("Reddit API error for r/{name}: {msg}").into());
        }

        let data = &resp["data"];
        let posts = data["children"].as_array().cloned().unwrap_or_default();
        if posts.is_empty() {
            break;
        }
        let posts_on_page = posts.len();
        let concurrency = cfg.batch_concurrency.clamp(1, 10);

        let tx = &doc_tx;
        futures_util::stream::iter(posts)
            .for_each_concurrent(concurrency, |post| async move {
                if let Some(doc) = build_post_doc(cfg, token, &post).await
                    && tx.send(doc).await.is_err()
                {
                    log_warn("command=ingest_reddit doc_channel_closed");
                }
            })
            .await;

        fetched_posts += posts_on_page;
        if max_posts > 0 && fetched_posts >= max_posts {
            break;
        }
        after = data["after"].as_str().unwrap_or("").to_string();
        if after.is_empty() {
            break;
        }
    }

    // Close the channel so the drain task finishes.
    drop(doc_tx);
    let total_count = drain_handle
        .await
        .map_err(|e| format!("reddit drain task failed: {e}"))?;
    Ok(total_count)
}

/// Flush a buffer of PreparedDocs to the embed pipeline in one batch call.
async fn flush_batch(cfg: &Config, buffer: &mut Vec<PreparedDoc>) -> usize {
    let batch = std::mem::take(buffer);
    match embed_prepared_docs(cfg, batch, None).await {
        Ok(summary) => summary.chunks_embedded,
        Err(e) => {
            log_warn(&format!("command=ingest_reddit batch_embed_failed err={e}"));
            0
        }
    }
}

/// Build a PreparedDoc for a single Reddit post (fetch content + comments).
/// Returns None if the post should be skipped (low score, empty content, etc.).
async fn build_post_doc(
    cfg: &Config,
    token: &str,
    post: &serde_json::Value,
) -> Option<PreparedDoc> {
    let data = &post["data"];
    let score = data["score"].as_i64().unwrap_or(0) as i32;
    if score < cfg.reddit_min_score {
        return None;
    }

    let title = data["title"].as_str().unwrap_or("Untitled");
    let selftext = data["selftext"].as_str().unwrap_or("");
    let permalink = data["permalink"].as_str().unwrap_or("");
    let post_url = format!("https://www.reddit.com{permalink}");

    let mut content = format!("# {title}");
    if !selftext.is_empty() {
        content.push_str(&format!("\n\n{selftext}"));
    }

    if !permalink.is_empty() {
        if cfg.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(cfg.delay_ms)).await;
        }
        match fetch_thread_comments(cfg, token, permalink).await {
            Ok(comments) => {
                format_comments_into(&mut content, title, &comments);
            }
            Err(e) => log_warn(&format!(
                "command=ingest_reddit fetch_comments_failed permalink={permalink} err={e}"
            )),
        }
    }

    if content.trim().is_empty() {
        return None;
    }

    let extra = meta::build_reddit_post_extra_payload(data);
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return None;
    }

    Some(PreparedDoc {
        url: post_url.clone(),
        domain: url_to_domain(&post_url),
        chunks,
        source_type: "reddit".to_string(),
        content_type: "text",
        title: Some(title.to_string()),
        extra: Some(extra),
    })
}

/// Embed a single Reddit thread (post + full recursive comment tree) by its URL.
async fn ingest_thread(
    cfg: &Config,
    token: &str,
    url: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    reporter.report_phase(PHASE_FETCHING_POSTS).await;
    let permalink = url
        .strip_prefix("https://www.reddit.com")
        .or_else(|| url.strip_prefix("https://old.reddit.com"))
        .or_else(|| url.strip_prefix("https://reddit.com"))
        .or_else(|| url.strip_prefix("http://www.reddit.com"))
        .or_else(|| url.strip_prefix("http://old.reddit.com"))
        .or_else(|| url.strip_prefix("http://reddit.com"))
        .unwrap_or(url);

    let json_url = format!(
        "https://oauth.reddit.com{}.json?limit=100&depth={}&raw_json=1",
        permalink.trim_end_matches('/'),
        cfg.reddit_depth
    );
    let resp = fetch_reddit_json(&json_url, token).await?;

    let post_data = &resp[0]["data"]["children"][0]["data"];
    let title = post_data["title"].as_str().unwrap_or("Reddit Thread");
    let selftext = post_data["selftext"].as_str().unwrap_or("");
    let permalink_field = post_data["permalink"].as_str().unwrap_or(permalink);
    let canonical_url = format!("https://www.reddit.com{permalink_field}");

    let mut content = format!("# {title}");
    if !selftext.is_empty() {
        content.push_str(&format!("\n\n{selftext}"));
    }

    if let Some(data) = resp[1].get("data") {
        let mut comments = Vec::new();
        collect_comments_recursive(
            data,
            1,
            cfg.reddit_depth,
            cfg.reddit_min_score,
            None,
            &mut comments,
        );
        format_comments_into(&mut content, title, &comments);
    }

    let extra = meta::build_reddit_post_extra_payload(post_data);
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return Ok(0);
    }
    let doc = PreparedDoc {
        url: canonical_url.clone(),
        domain: url_to_domain(&canonical_url),
        chunks,
        source_type: "reddit".to_string(),
        content_type: "text",
        title: Some(title.to_string()),
        extra: Some(extra),
    };
    let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
    Ok(summary.chunks_embedded)
}

/// Append formatted comments to the content string.
fn format_comments_into(content: &mut String, title: &str, comments: &[CommentWithContext]) {
    for comment_ctx in comments {
        let mut ctx = format!("\n\n---\nPost: {title}\n\n");
        if let Some(parent) = comment_ctx.parent_text.as_deref() {
            ctx.push_str(&format!("Replying to: {parent}\n\n"));
        }
        ctx.push_str(&comment_ctx.body);
        content.push_str(&ctx);
    }
}

/// Ingest Reddit content:
/// - For a subreddit: fetches posts (configurable sort/limit/score/depth) + recursive comments
/// - For a thread URL: fetches that thread + full recursive comment tree
/// - Embeds all content into Qdrant via PreparedDoc pipeline with reddit_* metadata
pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info(&format!("command=ingest source=reddit target={target}"));
    reporter.report_phase(PHASE_AUTHENTICATING).await;

    let client_id = cfg
        .reddit_client_id
        .as_deref()
        .ok_or("REDDIT_CLIENT_ID not configured (--reddit-client-id or env var)")?;
    let client_secret = cfg
        .reddit_client_secret
        .as_deref()
        .ok_or("REDDIT_CLIENT_SECRET not configured (--reddit-client-secret or env var)")?;

    let token = get_access_token(client_id, client_secret).await?;
    log_info("reddit oauth_acquired");

    let chunk_count = match classify_target(target) {
        RedditTarget::Subreddit(name) => ingest_subreddit(cfg, &token, &name, reporter).await?,
        RedditTarget::Thread(url) => ingest_thread(cfg, &token, &url, reporter).await?,
    };

    reporter
        .report(serde_json::json!({
            "phase": PHASE_EMBEDDING_POSTS,
            "chunks_embedded": chunk_count,
        }))
        .await;

    log_done(&format!(
        "command=ingest source=reddit target={target} chunk_count={chunk_count}"
    ));
    Ok(chunk_count)
}

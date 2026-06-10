mod client;
mod comments;
pub(crate) mod meta;
mod types;

pub use client::get_access_token;
pub use types::{
    RedditIngestOptions, RedditIngestStats, RedditIngestSummary, RedditTarget, classify_target,
};

use crate::core::config::{Config, RedditSort};
use crate::core::content::url_to_domain;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::progress::PhaseReporter;
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::sync::mpsc;

const PHASE_AUTHENTICATING: &str = "authenticating";
const PHASE_FETCHING_POSTS: &str = "fetching_posts";
const PHASE_EMBEDDING_POSTS: &str = "embedding_posts";

use client::fetch_reddit_json_with_cancel;
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
    options: &RedditIngestOptions,
) -> Result<RedditIngestSummary, Box<dyn Error>> {
    reporter.report_phase(PHASE_FETCHING_POSTS).await;
    validate_subreddit(name)?;

    use futures_util::StreamExt;

    let mut after = String::new();
    let mut fetched_posts = 0usize;
    let max_posts = cfg.reddit_max_posts;
    let comment_fetch_attempts = Arc::new(AtomicUsize::new(0));
    let comment_fetch_failures = Arc::new(AtomicUsize::new(0));
    let posts_prepared = Arc::new(AtomicUsize::new(0));

    let (doc_tx, drain_handle) = spawn_doc_drain(cfg);

    loop {
        options.check_cancelled()?;
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

        let resp = fetch_reddit_json_with_cancel(&url, token, options.cancel_token()).await?;
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
        let comment_fetch_attempts = Arc::clone(&comment_fetch_attempts);
        let comment_fetch_failures = Arc::clone(&comment_fetch_failures);
        let posts_prepared = Arc::clone(&posts_prepared);
        let options = options.clone();
        futures_util::stream::iter(posts)
            .for_each_concurrent(concurrency, |post| {
                let comment_fetch_attempts = Arc::clone(&comment_fetch_attempts);
                let comment_fetch_failures = Arc::clone(&comment_fetch_failures);
                let posts_prepared = Arc::clone(&posts_prepared);
                let options = options.clone();
                async move {
                    let doc_to_send = match build_post_doc(cfg, token, &post, &options).await {
                        Ok(PostBuildResult {
                            doc,
                            comment_fetch_attempted,
                            comment_fetch_failed,
                        }) => {
                            if comment_fetch_attempted {
                                comment_fetch_attempts.fetch_add(1, Ordering::Relaxed);
                            }
                            if comment_fetch_failed {
                                comment_fetch_failures.fetch_add(1, Ordering::Relaxed);
                            }
                            doc
                        }
                        Err(e) => {
                            log_warn(&format!("command=ingest_reddit post_skipped err={e}"));
                            None
                        }
                    };

                    if let Some(doc) = doc_to_send {
                        posts_prepared.fetch_add(1, Ordering::Relaxed);
                        if tx.send(doc).await.is_err() {
                            log_warn("command=ingest_reddit doc_channel_closed");
                        }
                    }
                }
            })
            .await;
        options.check_cancelled()?;

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
    Ok(RedditIngestSummary {
        chunks_embedded: total_count,
        stats: RedditIngestStats {
            posts_seen: fetched_posts,
            posts_prepared: posts_prepared.load(Ordering::Relaxed),
            comment_fetch_attempts: comment_fetch_attempts.load(Ordering::Relaxed),
            comment_fetch_failures: comment_fetch_failures.load(Ordering::Relaxed),
        },
    })
}

fn spawn_doc_drain(cfg: &Config) -> (mpsc::Sender<PreparedDoc>, tokio::task::JoinHandle<usize>) {
    let (doc_tx, mut doc_rx) = mpsc::channel::<PreparedDoc>(256);
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
        if !buffer.is_empty() {
            total_chunks += flush_batch(&drain_cfg, &mut buffer).await;
        }
        total_chunks
    });
    (doc_tx, drain_handle)
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
    options: &RedditIngestOptions,
) -> Result<PostBuildResult, Box<dyn Error>> {
    options.check_cancelled()?;
    let data = &post["data"];
    let score = data["score"].as_i64().unwrap_or(0) as i32;
    if score < cfg.reddit_min_score {
        return Ok(PostBuildResult::default());
    }

    let title = data["title"].as_str().unwrap_or("Untitled");
    let selftext = data["selftext"].as_str().unwrap_or("");
    let permalink = data["permalink"].as_str().unwrap_or("");
    let post_url = format!("https://www.reddit.com{permalink}");

    let mut content = format!("# {title}");
    if !selftext.is_empty() {
        content.push_str(&format!("\n\n{selftext}"));
    }

    let mut comment_fetch_attempted = false;
    let mut comment_fetch_failed = false;
    if !permalink.is_empty() {
        comment_fetch_attempted = true;
        if cfg.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(cfg.delay_ms)).await;
        }
        options.check_cancelled()?;
        match fetch_thread_comments(cfg, token, permalink, options.cancel_token()).await {
            Ok(comments) => {
                format_comments_into(&mut content, title, &comments);
            }
            Err(e) => {
                if options
                    .cancel_token()
                    .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
                {
                    return Err(e);
                }
                comment_fetch_failed = true;
                log_warn(&format!(
                    "command=ingest_reddit fetch_comments_failed permalink={permalink} err={e}"
                ));
            }
        }
    }

    if content.trim().is_empty() {
        return Ok(PostBuildResult {
            comment_fetch_attempted,
            comment_fetch_failed,
            ..PostBuildResult::default()
        });
    }

    let extra = meta::build_reddit_post_extra_payload(data);
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return Ok(PostBuildResult {
            comment_fetch_attempted,
            comment_fetch_failed,
            ..PostBuildResult::default()
        });
    }

    Ok(PostBuildResult {
        doc: Some(PreparedDoc::ingest(
            post_url.clone(),
            url_to_domain(&post_url),
            chunks,
            "reddit",
            Some(title.to_string()),
            Some(extra),
        )),
        comment_fetch_attempted,
        comment_fetch_failed,
    })
}

#[derive(Default)]
struct PostBuildResult {
    doc: Option<PreparedDoc>,
    comment_fetch_attempted: bool,
    comment_fetch_failed: bool,
}

/// Embed a single Reddit thread (post + full recursive comment tree) by its URL.
async fn ingest_thread(
    cfg: &Config,
    token: &str,
    permalink: &str,
    reporter: &PhaseReporter,
    options: &RedditIngestOptions,
) -> Result<RedditIngestSummary, Box<dyn Error>> {
    reporter.report_phase(PHASE_FETCHING_POSTS).await;
    options.check_cancelled()?;

    let json_url = format!(
        "https://oauth.reddit.com{}.json?limit=100&depth={}&raw_json=1",
        permalink.trim_end_matches('/'),
        cfg.reddit_depth
    );
    let resp = fetch_reddit_json_with_cancel(&json_url, token, options.cancel_token()).await?;

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
        return Ok(RedditIngestSummary {
            chunks_embedded: 0,
            stats: RedditIngestStats {
                posts_seen: 1,
                ..RedditIngestStats::default()
            },
        });
    }
    let doc = PreparedDoc::ingest(
        canonical_url.clone(),
        url_to_domain(&canonical_url),
        chunks,
        "reddit",
        Some(title.to_string()),
        Some(extra),
    );
    let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
    Ok(RedditIngestSummary {
        chunks_embedded: summary.chunks_embedded,
        stats: RedditIngestStats {
            posts_seen: 1,
            posts_prepared: 1,
            comment_fetch_attempts: 1,
            comment_fetch_failures: 0,
        },
    })
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
    Ok(
        ingest_reddit_with_options(cfg, target, reporter, &RedditIngestOptions::default())
            .await?
            .chunks_embedded,
    )
}

/// Ingest Reddit content with source-local control hooks and detailed stats.
pub async fn ingest_reddit_with_options(
    cfg: &Config,
    target: &str,
    reporter: &PhaseReporter,
    options: &RedditIngestOptions,
) -> Result<RedditIngestSummary, Box<dyn Error>> {
    log_info(&format!("command=ingest source=reddit target={target}"));
    reporter.report_phase(PHASE_AUTHENTICATING).await;
    options.check_cancelled()?;
    let target_kind = classify_target(target)?;

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
    options.check_cancelled()?;

    let summary = match target_kind {
        RedditTarget::Subreddit(name) => {
            ingest_subreddit(cfg, &token, &name, reporter, options).await?
        }
        RedditTarget::Thread(permalink) => {
            ingest_thread(cfg, &token, &permalink, reporter, options).await?
        }
    };

    reporter
        .report(serde_json::json!({
            "phase": PHASE_EMBEDDING_POSTS,
            "chunks_embedded": summary.chunks_embedded,
            "reddit_stats": {
                "posts_seen": summary.stats.posts_seen,
                "posts_prepared": summary.stats.posts_prepared,
                "comment_fetch_attempts": summary.stats.comment_fetch_attempts,
                "comment_fetch_failures": summary.stats.comment_fetch_failures,
                "partial_comment_failures": summary.stats.has_partial_comment_failures(),
            },
        }))
        .await;

    log_done(&format!(
        "command=ingest source=reddit target={target} chunk_count={} comment_fetch_failures={}",
        summary.chunks_embedded, summary.stats.comment_fetch_failures
    ));
    Ok(summary)
}

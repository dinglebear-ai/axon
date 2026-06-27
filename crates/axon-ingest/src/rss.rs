//! RSS / Atom feed ingestion.
//!
//! Fetches a feed document (RSS 0.9x/2.0, Atom 1.0, or JSON Feed), parses it
//! with `feed-rs`, and embeds one document per entry. Each entry's HTML content
//! (`content:encoded` / Atom `<content>` / `<summary>`) is converted to markdown
//! and embedded with the entry title, link, and publication date as metadata.
//!
//! Many feeds only publish truncated summaries; for full-text indexing of the
//! linked articles, pair this with `axon crawl`/`watch` on the article URLs.
//! The feed origin is recorded as `seed_url` (set by the ingest runner) so
//! `axon refresh` can re-pull the feed.

use std::collections::HashSet;
use std::error::Error;

use feed_rs::model::{Entry, Feed};
use futures_util::StreamExt;

use crate::progress::PhaseReporter;
use axon_core::config::Config;
use axon_core::content::{to_markdown, url_to_domain};
use axon_core::http::{http_client, normalize_url, validate_url};
use axon_core::logging::{log_done, log_info, log_warn};
use axon_vector::ops::{PreparedDoc, embed_prepared_docs, prepare_plain_text_source};

/// Maximum feed document size accepted (16 MiB). Feeds are link indexes, not
/// content dumps; anything larger is almost certainly not a feed.
const MAX_FEED_BYTES: usize = 16 * 1024 * 1024;

/// Maximum number of entries embedded from a single feed.
const MAX_FEED_ENTRIES: usize = 500;

const PHASE_FETCHING: &str = "fetching";
const PHASE_PARSING: &str = "parsing";
const PHASE_EMBEDDING: &str = "embedding";

/// Ingest an RSS/Atom/JSON feed into the vector store. Returns the number of
/// chunks embedded across all entries.
pub async fn ingest_rss(
    cfg: &Config,
    url: &str,
    reporter: &PhaseReporter,
) -> Result<usize, Box<dyn Error>> {
    log_info(&format!("command=ingest source=rss target={url}"));
    validate_url(url)?;

    reporter.report_phase(PHASE_FETCHING).await;
    let bytes = fetch_feed_bytes(url).await?;

    reporter.report_phase(PHASE_PARSING).await;
    let feed = feed_rs::parser::parse(&bytes[..])
        .map_err(|e| format!("failed to parse feed at {url}: {e}"))?;
    let feed_title = feed.title.as_ref().map(|t| t.content.clone());

    let docs = prepare_feed_docs(url, feed_title.as_deref(), &feed);
    if docs.is_empty() {
        return Err(format!("feed at {url} contained no embeddable entries").into());
    }
    let entry_count = docs.len();

    reporter.report_phase(PHASE_EMBEDDING).await;
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| format!("rss embed failed for {url}: {e}"))?
        .require_success("rss embed")?;

    log_done(&format!(
        "command=ingest source=rss target={url} entries={entry_count} chunk_count={}",
        summary.chunks_embedded
    ));
    Ok(summary.chunks_embedded)
}

/// Fetch the feed document, enforcing the size cap while streaming so a hostile
/// or misconfigured endpoint can't OOM us by returning a multi-GB body before
/// the cap is checked.
async fn fetch_feed_bytes(url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let client = http_client()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    // Reject early when the server advertises an over-cap body.
    if let Some(len) = resp.content_length()
        && len > MAX_FEED_BYTES as u64
    {
        return Err(format!(
            "feed at {url} advertises {len} bytes, exceeds {MAX_FEED_BYTES} byte cap"
        )
        .into());
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > MAX_FEED_BYTES {
            return Err(
                format!("feed at {url} exceeds {MAX_FEED_BYTES} byte cap while streaming").into(),
            );
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Build one `PreparedDoc` per feed entry that has usable content.
fn prepare_feed_docs(feed_url: &str, feed_title: Option<&str>, feed: &Feed) -> Vec<PreparedDoc> {
    if feed.entries.len() > MAX_FEED_ENTRIES {
        log_warn(&format!(
            "feed {feed_url} has {} entries; embedding only the first {MAX_FEED_ENTRIES}",
            feed.entries.len()
        ));
    }
    let mut docs = Vec::new();
    let mut seen_entry_identities = HashSet::new();
    for entry in feed.entries.iter().take(MAX_FEED_ENTRIES) {
        match prepare_entry_doc(feed_url, feed_title, entry) {
            Some((identity, doc)) => {
                if seen_entry_identities.insert(identity.clone()) {
                    docs.push(doc);
                } else {
                    log_info(&format!(
                        "skipping duplicate feed entry link: feed={feed_url} url={} identity={identity}",
                        doc.url()
                    ));
                }
            }
            None => log_warn(&format!(
                "skipping feed entry with no link or content (id={})",
                entry.id
            )),
        }
    }
    docs
}

fn normalized_entry_link_identity(link: &str) -> String {
    let normalized = normalize_url(link).into_owned();
    let Ok(mut url) = reqwest::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_fragment(None);
    let retained: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_param(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if retained.is_empty() {
        url.set_query(None);
    } else {
        let query = retained
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        url.set_query(Some(&query));
    }
    url.to_string()
}

fn is_tracking_query_param(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.starts_with("utm_")
        || matches!(
            lower.as_str(),
            "gclid" | "fbclid" | "mc_cid" | "mc_eid" | "igshid" | "ref"
        )
}

/// Convert a single feed entry into a `PreparedDoc`, or `None` when it has no
/// resolvable link and no body to embed.
fn prepare_entry_doc(
    feed_url: &str,
    feed_title: Option<&str>,
    entry: &Entry,
) -> Option<(String, PreparedDoc)> {
    let link = entry_link(entry)?;
    let canonical_link = normalized_entry_link_identity(&link);
    let title = entry
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .filter(|s| !s.is_empty());

    let body_html = entry
        .content
        .as_ref()
        .and_then(|c| c.body.clone())
        .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
        .unwrap_or_default();
    let body_md = to_markdown(&body_html, None);
    let body_md = body_md.trim();

    // Compose a small heading so the title is searchable even for summary-only
    // feeds. Skip entries that have neither a title nor any body content.
    let text = match (title.as_deref(), body_md.is_empty()) {
        (Some(t), false) => format!("# {t}\n\n{body_md}"),
        (Some(t), true) => format!("# {t}"),
        (None, false) => body_md.to_string(),
        (None, true) => return None,
    };

    let published = entry.published.or(entry.updated).map(|dt| dt.to_rfc3339());
    let author = entry
        .authors
        .first()
        .map(|p| p.name.clone())
        .filter(|s| !s.is_empty());
    let extra = serde_json::json!({
        "feed_url": feed_url,
        "feed_title": feed_title,
        "entry_id": entry.id,
        "entry_link": link,
        "published": published,
        "author": author,
    });

    Some((
        canonical_link.clone(),
        prepare_plain_text_source(
            canonical_link.clone(),
            url_to_domain(&canonical_link),
            text,
            "rss",
            title,
            Some(extra),
        ),
    ))
}

/// Resolve the canonical link for an entry: prefer an `alternate` link, then the
/// first link, then the entry id when it is itself an absolute URL.
fn entry_link(entry: &Entry) -> Option<String> {
    if let Some(alt) = entry
        .links
        .iter()
        .find(|l| l.rel.as_deref() == Some("alternate"))
    {
        return Some(alt.href.clone());
    }
    if let Some(first) = entry.links.first() {
        return Some(first.href.clone());
    }
    if entry.id.starts_with("http://") || entry.id.starts_with("https://") {
        return Some(entry.id.clone());
    }
    None
}

#[cfg(test)]
#[path = "rss_tests.rs"]
mod tests;

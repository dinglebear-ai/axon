//! Feed entry extraction — ported from the legacy `axon-ingest::rss` parser,
//! minus the network fetch (the adapter reads an already-prepared feed
//! document from disk) and minus the vector-pipeline `PreparedDoc` coupling.

use feed_rs::model::{Entry, Feed};

/// A single parsed feed entry, reduced to the fields the adapter needs to
/// build a `ManifestItem` / `SourceDocument`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntry {
    pub entry_id: String,
    pub link: String,
    pub title: Option<String>,
    pub body_html: String,
    pub published: Option<String>,
    pub author: Option<String>,
}

/// Parse raw feed bytes (RSS 0.9x/2.0, Atom 1.0, or JSON Feed) into a `Feed`.
pub fn parse_feed_bytes(bytes: &[u8]) -> Result<Feed, String> {
    feed_rs::parser::parse(bytes).map_err(|err| err.to_string())
}

/// Extract entries with a resolvable link, deduplicated by normalized link
/// identity (first occurrence wins), matching the legacy ingest behavior of
/// skipping entries with neither a link nor an id-as-url.
pub fn extract_entries(feed: &Feed) -> Vec<FeedEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    for entry in &feed.entries {
        let Some(link) = entry_link(entry) else {
            continue;
        };
        let identity = normalized_link_identity(&link);
        if !seen.insert(identity) {
            continue;
        }
        entries.push(build_entry(entry, link));
    }
    entries
}

fn build_entry(entry: &Entry, link: String) -> FeedEntry {
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
    let published = entry.published.or(entry.updated).map(|dt| dt.to_rfc3339());
    let author = entry
        .authors
        .first()
        .map(|p| p.name.clone())
        .filter(|s| !s.is_empty());
    FeedEntry {
        entry_id: entry.id.clone(),
        link,
        title,
        body_html,
        published,
        author,
    }
}

/// Resolve the canonical link for an entry: prefer an `alternate` link, then
/// the first link, then the entry id when it is itself an absolute URL.
pub fn entry_link(entry: &Entry) -> Option<String> {
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

/// Normalize a link for de-duplication identity: strip fragment + tracking
/// query params. Falls back to the raw link when it doesn't parse as a URL.
fn normalized_link_identity(link: &str) -> String {
    let Ok(mut url) = url::Url::parse(link) else {
        return link.to_string();
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

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;

//! Diff service: fetch two URLs and compare their content.
//!
//! The pure computation (`compute_diff`) is separated from I/O (`diff`) so it
//! can be tested without network calls.

use std::collections::HashSet;
use std::error::Error;

use similar::TextDiff;
use tokio::sync::mpsc;

use crate::core::config::Config;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::scrape;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry, MetadataChange};

/// Fetch `url_a` and `url_b`, then compute and return a `DiffResult`.
pub async fn diff(
    cfg: &Config,
    url_a: &str,
    url_b: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DiffResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("diff: fetching {url_a} and {url_b}"),
        },
    )
    .await;

    let results =
        scrape::scrape_batch(cfg, &[url_a.to_string(), url_b.to_string()], tx.clone()).await?;

    let (doc_a, doc_b) = match results.as_slice() {
        [a, b] => (a, b),
        _ => {
            return Err("diff requires exactly two URLs to be fetched successfully".into());
        }
    };

    let links_a = extract_links_from_payload(&doc_a.payload);
    let links_b = extract_links_from_payload(&doc_b.payload);

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "diff: computing changes".to_string(),
        },
    )
    .await;

    Ok(compute_diff(
        &doc_a.url,
        &doc_a.markdown,
        &links_a,
        &doc_a.payload,
        &doc_b.url,
        &doc_b.markdown,
        &links_b,
        &doc_b.payload,
    ))
}

/// Pure diff computation — no I/O.
///
/// Exposed as `pub(crate)` so sidecar tests can call it directly without
/// requiring network access.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_diff(
    url_a: &str,
    markdown_a: &str,
    links_a: &[LinkEntry],
    meta_a: &serde_json::Value,
    url_b: &str,
    markdown_b: &str,
    links_b: &[LinkEntry],
    meta_b: &serde_json::Value,
) -> DiffResult {
    let text_diff = compute_text_diff(markdown_a, markdown_b);
    let metadata_changes = compute_metadata_changes(meta_a, meta_b);
    let (links_added, links_removed) = compute_link_changes(links_a, links_b);
    let word_count_a = markdown_a.split_whitespace().count() as i64;
    let word_count_b = markdown_b.split_whitespace().count() as i64;
    let word_count_delta = word_count_b - word_count_a;

    let status = if text_diff.is_none() && metadata_changes.is_empty() {
        DiffStatus::Same
    } else {
        DiffStatus::Changed
    };

    DiffResult {
        url_a: url_a.to_string(),
        url_b: url_b.to_string(),
        status,
        text_diff,
        metadata_changes,
        links_added,
        links_removed,
        word_count_delta,
    }
}

fn compute_text_diff(old: &str, new: &str) -> Option<String> {
    if old == new {
        return None;
    }
    let d = TextDiff::from_lines(old, new);
    let unified = d
        .unified_diff()
        .context_radius(3)
        .header("a", "b")
        .to_string();
    if unified.is_empty() {
        None
    } else {
        Some(unified)
    }
}

const COMPARED_META_FIELDS: &[&str] = &[
    "title",
    "description",
    "author",
    "published_date",
    "language",
    "url",
    "site_name",
    "image",
    "favicon",
];

fn compute_metadata_changes(
    meta_a: &serde_json::Value,
    meta_b: &serde_json::Value,
) -> Vec<MetadataChange> {
    let mut changes = Vec::new();
    for &field in COMPARED_META_FIELDS {
        let old = meta_a
            .get(field)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let new = meta_b
            .get(field)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if old != new {
            changes.push(MetadataChange {
                field: field.to_string(),
                old,
                new,
            });
        }
    }
    changes
}

fn compute_link_changes(
    links_a: &[LinkEntry],
    links_b: &[LinkEntry],
) -> (Vec<LinkEntry>, Vec<LinkEntry>) {
    let hrefs_a: HashSet<&str> = links_a.iter().map(|l| l.href.as_str()).collect();
    let hrefs_b: HashSet<&str> = links_b.iter().map(|l| l.href.as_str()).collect();

    let added = links_b
        .iter()
        .filter(|l| !hrefs_a.contains(l.href.as_str()))
        .cloned()
        .collect();
    let removed = links_a
        .iter()
        .filter(|l| !hrefs_b.contains(l.href.as_str()))
        .cloned()
        .collect();
    (added, removed)
}

/// Extract links from a scrape payload's `links` field if present.
fn extract_links_from_payload(payload: &serde_json::Value) -> Vec<LinkEntry> {
    payload
        .get("links")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let href = item.get("href")?.as_str()?.to_string();
                    let text = item
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(LinkEntry { href, text })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;

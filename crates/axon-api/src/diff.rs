//! Transport-neutral diff result DTOs + the pure diff-computation functions.
//!
//! Shared by the `services::diff` compute path, the HTTP `/v1/diff` route, and
//! the jobs watch scheduler's change detector. Living here lets `axon-jobs`
//! consume them without depending on `axon-services`.

use similar::TextDiff;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Same,
    Changed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct MetadataChange {
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct LinkEntry {
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct DiffResult {
    pub url_a: String,
    pub url_b: String,
    pub status: DiffStatus,
    /// Unified diff of the markdown content, if any changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_diff: Option<String>,
    pub metadata_changes: Vec<MetadataChange>,
    pub links_added: Vec<LinkEntry>,
    pub links_removed: Vec<LinkEntry>,
    pub word_count_delta: i64,
}

/// Compute a structured diff between two scraped snapshots (text + metadata + links).
#[allow(clippy::too_many_arguments)]
pub fn compute_diff(
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

    let status = if text_diff.is_none()
        && metadata_changes.is_empty()
        && links_added.is_empty()
        && links_removed.is_empty()
    {
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
pub fn extract_links_from_payload(payload: &serde_json::Value) -> Vec<LinkEntry> {
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

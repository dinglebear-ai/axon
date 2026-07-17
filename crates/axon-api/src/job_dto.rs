//! Transport-neutral scrape / extract result DTOs shared by the
//! services layer and the jobs layer (runners + watch change-detector).

use crate::result::DocumentBackend;
use crate::source::ArtifactHandle;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScrapeResult {
    pub payload: serde_json::Value,
    pub url: String,
    pub markdown: String,
    pub output: String,
    pub artifact_handle: Option<ArtifactHandle>,

    // Document windowing fields
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_tokens_estimate: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<DocumentBackend>,
    /// URLs the extractor recommends crawling as a follow-up (e.g. docs.rs for
    /// a crates.io crate). Empty for generic scrapes and most verticals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata (from `ScrapedDoc.extra`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    /// Redacted and size-capped structured data summary from a vertical extractor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
    /// Redacted structured data retained for embedding. This is intentionally
    /// not part of the wire response; public `structured` remains size-capped.
    #[serde(skip)]
    pub structured_for_embedding: Option<serde_json::Value>,
    /// Vertical extractor name (from `ScrapedDoc.extractor_name`). None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extractor_name: Option<String>,
    /// Page title from the vertical extractor. None for generic scrapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractSyncResult {
    pub summary: serde_json::Value,
    pub summary_path: String,
    pub items_path: String,
    pub total_items: usize,
    pub duration_ms: u128,
}

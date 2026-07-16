//! Shared types returned by vertical extractors.

/// Output of a successful vertical extraction.
///
/// Carries enough information to build a `PreparedDoc` for embedding.
/// The `extractor_name` + `extractor_version` fields flow through to the
/// Qdrant payload so retrieval can filter by source extractor.
#[derive(Debug, Clone)]
pub struct ScrapedDoc {
    pub url: String,
    pub markdown: String,
    pub title: Option<String>,
    /// Stable extractor identifier (e.g. `"github_repo"`, `"pypi"`).
    pub extractor_name: &'static str,
    /// Monotone version bump when extraction logic changes in a
    /// backward-incompatible way (triggers reindex on upgrade).
    pub extractor_version: u32,
    /// Optional structured-data blob (JSON-LD, API response fragment).
    pub structured: Option<serde_json::Value>,
    /// URLs the caller should crawl after embedding this doc (e.g. the docs
    /// site for a crate). Empty for most verticals. Propagated to `ScrapeResult`.
    pub follow_crawl_urls: Vec<String>,
    /// Curated per-extractor metadata fields to merge flat into the Qdrant payload.
    /// Every key becomes a top-level payload field when embedded.
    /// Keys must follow the prefix convention: `pkg_*`, `git_*`, `hf_*`, etc.
    /// Absent beats null — only set keys that have actual values.
    pub extra: Option<serde_json::Value>,
}

/// Static descriptor exported by one vertical extractor implementation.
#[derive(Debug, Clone)]
pub struct ExtractorInfo {
    /// Stable machine-readable implementation name.
    pub name: &'static str,
    /// Human-readable label for `axon scrape --list-verticals`.
    pub label: &'static str,
    /// One-sentence description.
    pub description: &'static str,
    /// URL patterns this extractor claims (for documentation / discovery).
    pub url_patterns: &'static [&'static str],
    /// Whether an adapter may include this implementation in automatic URL
    /// dispatch. `false` reserves it for an explicit adapter request.
    pub auto_dispatch: bool,
}

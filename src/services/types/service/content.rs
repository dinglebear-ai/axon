use crate::services::types::client_server::ArtifactHandle;

use super::query::DocumentBackend;
// ── Scrape / map / search / research ─────────────────────────────────────────

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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeDocument {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub content_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeTiming {
    pub scrape: u128,
    pub llm: u128,
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SummarizeResult {
    pub urls: Vec<String>,
    pub documents: Vec<SummarizeDocument>,
    pub summary: String,
    pub context_chars: usize,
    pub context_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<SummarizeUsage>,
    pub timing_ms: SummarizeTiming,
}

/// Typed result of a `map` (URL discovery) operation.
///
/// Replaces the previous `MapResult { payload: serde_json::Value }` pass-through.
/// Serializes to the same JSON shape so callers that use `serde_json::to_value`
/// remain wire-compatible.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapResult {
    /// The start URL that was mapped.
    pub url: String,
    /// Number of URLs returned in this page slice (equals `urls.len()`).
    ///
    /// This is the post-pagination count. For the pre-pagination total
    /// across all discovered URLs, use [`MapResult::total`]. The wire JSON
    /// key remains `mapped_urls` for backward compatibility.
    #[serde(rename = "mapped_urls")]
    pub returned_url_count: u64,
    /// Pre-pagination total URL count (all discovered URLs before offset/limit).
    ///
    /// CLI callers always pass `limit=0, offset=0`, so `total == mapped_urls` there.
    /// MCP callers use this field for `total_urls` in the response to avoid
    /// reporting the paginated count as the total.
    pub total: u64,
    /// Raw count of URLs found in sitemap.xml (before dedup).
    pub sitemap_urls: usize,
    /// Number of pages actually fetched during a crawl pass (0 in sitemap-only mode).
    pub pages_seen: u32,
    /// Pages below the minimum markdown character threshold.
    pub thin_pages: u32,
    /// Wall-clock time for the entire map operation.
    pub elapsed_ms: u64,
    /// How the URLs were discovered: `"sitemap"`, `"crawl"`, or `"bounded-structure"`.
    pub map_source: String,
    /// Optional user-visible warning (e.g. too few URLs found).
    pub warning: Option<String>,
    /// The discovered URLs (deduplicated, sorted), after offset/limit pagination.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub results: Vec<serde_json::Value>,
}

/// Origin of the synthesized summary returned by `research`.
///
/// `Llm` means the LLM produced the summary; `Fallback` means synthesis
/// failed and a deterministic snippet-based summary was substituted;
/// `None` means no extractions were available to summarize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SummarySource {
    Llm,
    Fallback,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchHit {
    pub position: usize,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    OfficialDocs,
    ReferenceDocs,
    Repository,
    Blog,
    Forum,
    News,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReputation {
    Authoritative,
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInstructionTrust {
    EvidenceOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchExtraction {
    pub url: String,
    pub title: String,
    pub extracted: String,
    pub source_type: SourceType,
    pub source_reputation: SourceReputation,
    pub instruction_trust: SourceInstructionTrust,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance_score: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchCrawlJob {
    pub url: String,
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchCrawlRejection {
    pub url: Option<String>,
    pub position: Option<i64>,
    pub title: Option<String>,
    pub kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ResearchUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchTiming {
    pub total: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchPayload {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
    pub search_results: Vec<ResearchHit>,
    pub extractions: Vec<ResearchExtraction>,
    pub auto_crawl_status: String,
    pub crawl_jobs: Vec<ResearchCrawlJob>,
    pub crawl_jobs_rejected: Vec<ResearchCrawlRejection>,
    pub summary: Option<String>,
    pub summary_source: SummarySource,
    pub usage: ResearchUsage,
    pub timing_ms: ResearchTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchResult {
    pub payload: ResearchPayload,
}
